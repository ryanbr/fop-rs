//! One FOP at a time per repository.
//!
//! The lists are sorted by a loop that runs 25 scripts over a single clone, two
//! of them often at once from different screens. Nothing in FOP or in git
//! serialises that, and a measured 12 concurrent pairs produced: 12 commits
//! instead of 24, ten `index.lock` collisions, nine `cannot rebase: You have
//! unstaged changes`, three repositories left wedged on a stale
//! `.git/rebase-merge`, and one appended domain lost outright -- absent from
//! HEAD, from the working tree and from every commit on every branch, the
//! autostash inside the other run's sync having swallowed it. The same 12 pairs
//! serialised produced 24 of 24 with no git error and a clean tree.
//!
//! So FOP takes an exclusive lock on the repository for the span that reads,
//! rewrites, commits and pushes the lists. `File::lock` is std's, so this needs
//! no dependency and no `cfg(windows)` arm: it is `flock` on unix and
//! `LockFileEx` on Windows, and both release when the handle closes, including
//! on a crash. There is no stale lock to clear.
//!
//! The wait is bounded. `http.lowSpeedLimit` is 0 and `http.lowSpeedTime`
//! 999999 in the environment this runs in, so git never aborts a stalled
//! transfer and a push can hold the lock indefinitely. An unbounded wait would
//! turn one stalled push into two frozen screens instead of one. Timing out is
//! safe because the append happens in the script, before FOP is invoked: a run
//! that gives up leaves the new rule in the working tree, and the next run from
//! any script sorts and commits it. Nothing is lost, one publish is deferred.
//!
//! The lock file is `fop.lock` inside the git directory, deliberately not a
//! path a wrapper script would pick. The scripts hold their own
//! `flock` on a publish lock before they append, and `flock` is not reentrant:
//! a child that opens the same file blocks on its parent's lock for ever. A
//! separate path is what keeps FOP from deadlocking against the script that
//! launched it.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long to wait for another FOP before giving up. Generous against a normal
/// hold, which measures a fraction of a second plus the push, and far short of
/// the interval between publish runs.
const WAIT_LIMIT: Duration = Duration::from_secs(600);

/// `File::lock` blocks with no way to time out, so the wait polls `try_lock`.
/// The interval starts short, so the common case of a hold that is already
/// ending costs almost nothing, and backs off to keep a long wait cheap.
const POLL_FIRST: Duration = Duration::from_millis(50);
const POLL_MAX: Duration = Duration::from_millis(500);

/// A held repository lock. Dropping it releases the lock.
pub struct RepoLock {
    _file: File,
}

/// What [`acquire`] settled on.
pub enum LockOutcome {
    /// The lock is held for as long as the value lives.
    Held(RepoLock),
    /// Nothing to lock: no repository, or a filesystem that cannot lock. The
    /// caller carries on -- the lock is protection against a second FOP, not a
    /// precondition for sorting.
    NotNeeded,
    /// Another FOP held the lock for the whole wait. The caller must stop
    /// rather than proceed unserialised: proceeding is the race this exists to
    /// prevent, and stopping only defers the work.
    TimedOut,
}

/// Resolve the git directory for a working tree.
///
/// `.git` is a directory in an ordinary clone and a file holding
/// `gitdir: <path>` in a worktree or submodule. The second form matters because
/// every worktree of one clone shares the same object store and index-adjacent
/// state, so each must land on its own lock rather than all taking the main
/// clone's.
pub(crate) fn git_dir(location: &Path) -> Option<PathBuf> {
    let dot_git = location.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    // A worktree's `.git` file. Relative paths in it are relative to the
    // working tree, so join rather than trust it as absolute.
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let rest = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    if rest.is_empty() {
        return None;
    }
    let path = Path::new(rest);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        location.join(path)
    };
    resolved.is_dir().then_some(resolved)
}

/// Take the repository lock, waiting a bounded time for any other FOP.
pub fn acquire(location: &Path, quiet: bool) -> LockOutcome {
    acquire_with_limit(location, quiet, WAIT_LIMIT)
}

/// [`acquire`] with the wait spelled out, so the limit can be driven from a
/// test without one sitting there for the real ten minutes.
pub fn acquire_with_limit(location: &Path, quiet: bool, wait_limit: Duration) -> LockOutcome {
    let Some(dir) = git_dir(location) else {
        return LockOutcome::NotNeeded;
    };
    let path = dir.join("fop.lock");
    let file = match OpenOptions::new().create(true).write(true).truncate(false).open(&path) {
        Ok(f) => f,
        // An unwritable git directory is not a reason to refuse to sort: a
        // read-only checkout has no commit to race with in the first place.
        Err(e) => {
            if !quiet {
                eprintln!("Could not open {}: {} -- continuing without the repository lock.", path.display(), e);
            }
            return LockOutcome::NotNeeded;
        }
    };

    let deadline = Instant::now() + wait_limit;
    let mut poll = POLL_FIRST;
    let mut announced = false;
    loop {
        match file.try_lock() {
            Ok(()) => return LockOutcome::Held(RepoLock { _file: file }),
            Err(TryLockError::WouldBlock) => {
                // Say so once, before the first sleep, so a wait reads as a
                // wait rather than a hang.
                if !announced {
                    if !quiet {
                        eprintln!(
                            "Another FOP is working in this repository. Waiting up to {}s for it to finish...",
                            wait_limit.as_secs()
                        );
                    }
                    // Set whether or not it was printed: this records that the
                    // wait was entered, which a quiet run does just the same.
                    announced = true;
                }
                let now = Instant::now();
                if now >= deadline {
                    // Not gated on `quiet`, matching every other fatal exit in
                    // FOP: quiet suppresses progress, and a run that stops
                    // having done nothing is not progress. Silence here would
                    // leave a skipped publish with no trace but an exit code,
                    // and nothing in the scripts reads one.
                    eprintln!(
                        "Another FOP has held {} for {}s. Stopping without sorting or committing.",
                        path.display(),
                        wait_limit.as_secs()
                    );
                    eprintln!(
                        "Anything already written to the lists stays in the working tree and the next run will pick it up. \
Check for a stalled push, or pass --no-repo-lock to go ahead unserialised."
                    );
                    return LockOutcome::TimedOut;
                }
                std::thread::sleep(poll.min(deadline - now));
                poll = (poll * 2).min(POLL_MAX);
            }
            // Locking unsupported on this filesystem, which some network mounts
            // report. Sorting is still correct; it just is not serialised.
            Err(TryLockError::Error(e)) if e.kind() == io::ErrorKind::Unsupported => {
                if !quiet {
                    eprintln!("This filesystem does not support locking -- continuing without the repository lock.");
                }
                return LockOutcome::NotNeeded;
            }
            Err(TryLockError::Error(e)) => {
                if !quiet {
                    eprintln!("Could not take the repository lock: {} -- continuing without it.", e);
                }
                return LockOutcome::NotNeeded;
            }
        }
    }
}
