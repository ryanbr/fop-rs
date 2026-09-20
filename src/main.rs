//! FOP - Filter Orderer and Preener
//!
//! A tool for sorting and cleaning ad-blocking filter lists.
//! Rust port of the original Python FOP by Michael (EasyList project).
//!
//! Copyright (C) 2025 FanboyNZ (FOP Rust)
//! https://github.com/ryanbr/fop-rs
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

#![allow(clippy::write_with_newline)]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Upper bound on rayon workers. Past this, extra workers cost a mimalloc heap
/// each without improving throughput -- see the pool setup in `main`.
const MAX_WORKERS: usize = 8;

/// Resolve the worker count from the explicit setting, the environment and the
/// machine, in that order.
///
/// Shared so `--show-config` reports the pool that will actually be built
/// rather than re-deriving it and disagreeing.
fn resolve_workers(explicit: Option<usize>) -> (usize, &'static str) {
    // Clamped here rather than at each source: the CLI and the environment
    // both clamped, `.fopconfig` did not, and a typo there -- where one lives
    // longest -- asked rayon for a pool of that size.
    if let Some(n) = explicit {
        return (n.min(MAX_THREADS), "set");
    }
    // Only claim the variable as the source when its value was actually usable:
    // an empty or malformed one falls through to the machine, and saying
    // otherwise sends someone looking at an environment that had no effect.
    let from_env = std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .map(|n| n.min(MAX_THREADS));
    if let Some(n) = from_env {
        return (n, "RAYON_NUM_THREADS");
    }
    let auto = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(MAX_WORKERS);
    (auto, "auto")
}

/// Ceiling on an explicit `--threads`/`RAYON_NUM_THREADS`. The cap above is a
/// default, not a limit -- asking for more is legitimate, and oversubscribing a
/// core is a normal thing to want. This only stops a typo (`--threads=1000000`)
/// from trying to spawn a thread per digit.
const MAX_THREADS: usize = 1024;

mod fop_git;
mod fop_checksum;
mod fop_sort;
mod fop_rules;
mod fop_typos;
mod fop_datestamp;

#[cfg(test)]
mod tests;

use ahash::AHashMap;
use ahash::AHashSet as HashSet;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use owo_colors::OwoColorize;

use std::sync::LazyLock;
/// Thread-safe warning output
pub(crate) static WARNING_BUFFER: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(Vec::with_capacity(100)));
pub(crate) static WARNING_OUTPUT: LazyLock<Mutex<Option<PathBuf>>> =
    LazyLock::new(|| Mutex::new(None));
/// Fast flag to avoid mutex lock on every write_warning call
pub(crate) static WARNING_TO_FILE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// Counter for files with Windows line endings (CRLF)
pub(crate) static CRLF_FILES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Get user's home directory (cross-platform)
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Whether the working directory is the home directory, where `./.fopconfig`
/// is the user's own `~/.fopconfig`.
fn cwd_is_home() -> bool {
    let canonical = |p: PathBuf| fs::canonicalize(p).ok();
    match (std::env::current_dir().ok().and_then(canonical), home_dir().and_then(canonical)) {
        (Some(cwd), Some(home)) => cwd == home,
        _ => false,
    }
}

/// A relative path that stays inside the working directory: no `..` or root
/// in its text, and -- since a folder along the way could be a symlink out of
/// the tree -- a parent directory that resolves inside it. The file itself is
/// guarded where it is written (`create_file_no_follow`).
fn stays_in_tree(path: &Path) -> bool {
    std::env::current_dir().is_ok_and(|cwd| stays_in_tree_of(path, &cwd))
}

/// `stays_in_tree`, resolved against `base` rather than the working directory.
fn stays_in_tree_of(path: &Path, base: &Path) -> bool {
    let lexical = path.is_relative()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir));
    if !lexical {
        return false;
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    match (fs::canonicalize(base.join(parent)), fs::canonicalize(base)) {
        (Ok(dir), Ok(base)) => dir.starts_with(base),
        _ => false,
    }
}

/// Limit what a `.fopconfig` in the working directory may do. It travels with
/// the repository, so it may be someone else's -- a checked-out pull request
/// can add one -- and it could otherwise name the program FOP runs as git, or
/// aim a write at any file. It keeps every sorting choice. `from_config` holds
/// what that file set; a value the command line replaced is not judged, and
/// `--config-file` and `~/.fopconfig` are the user's own and never come here.
/// An `Err` is fatal: output-diff also means "change nothing", and dropping it
/// would sort and rewrite the files instead.
fn restrict_repo_config(
    git_binary: &mut Option<String>,
    warning_output: &mut Option<PathBuf>,
    output_diff: &Option<PathBuf>,
    from_config: (Option<String>, Option<PathBuf>, Option<PathBuf>),
) -> Result<(), String> {
    let (config_git_binary, config_warning_output, config_output_diff) = from_config;
    if config_git_binary.is_some() && *git_binary == config_git_binary {
        eprintln!(
            "Warning: ignoring git-binary in ./.fopconfig: a repository's own config may not \
             choose the program FOP runs. Use --git-binary or ~/.fopconfig."
        );
        *git_binary = None;
    }
    if let Some(path) = config_warning_output.filter(|p| !stays_in_tree(p)) {
        if warning_output.as_deref() == Some(path.as_path()) {
            eprintln!(
                "Warning: ignoring warning-output = {} in ./.fopconfig: a repository's own config \
                 may only name a file inside this directory. Warnings go to stderr.",
                path.display()
            );
            *warning_output = None;
        }
    }
    if let Some(path) = config_output_diff.filter(|p| !stays_in_tree(p)) {
        if output_diff.as_deref() == Some(path.as_path()) {
            return Err(format!(
                "output-diff = {} in ./.fopconfig points outside this directory. A repository's \
                 own config may only name a file inside it; pass --output-diff to choose.",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Get current git user name
fn get_git_username() -> Option<String> {
    std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_lowercase())
}

/// A path-valued `.fopconfig` option, where an empty value means "not set".
///
/// `PathBuf::from("")` is not nothing: it made each of these options present
/// with an unusable path. A bare `warning-output =` -- as the README's sample
/// config ships it -- sent every warning to a file that was never written; an
/// empty `output-diff` switched on dry-run so nothing was sorted; and an empty
/// `check-banned-list` warned on every run that it could not load "".
fn non_empty_path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    (!value.is_empty()).then(|| PathBuf::from(value))
}

/// A path given on the command line, where an empty value is an error.
///
/// Not the same rule as `.fopconfig`: a blank config line is a key left unset,
/// but `--output-diff=` on the command line is nearly always a script expanding
/// an unset variable, and the flag still says what was wanted. Dropping it
/// widens the run instead of narrowing it -- an empty `--output-diff=` asked for
/// a read-only diff and got every file rewritten; an empty `--check-file=` asked
/// for one file and got the whole repository sorted. So it stops, as a
/// malformed `--threads` or `--commit-mask` does.
fn cli_path(flag: &str, value: &str) -> PathBuf {
    non_empty_path(value).unwrap_or_else(|| {
        eprintln!("Error: {} needs a path (got an empty value)", flag);
        std::process::exit(2);
    })
}

thread_local! {
    /// Set while `fop_sort::tidy_rule` runs. See `SuppressWarnings`.
    static WARNINGS_SUPPRESSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Silences `write_warning` on this thread until dropped.
///
/// The rule checks put each added line through the sort's own tidying to judge
/// it as it will be written, and that tidying warns -- "Removed invalid
/// domain(s)", "option ... is not recognised". The sort then tidies the same
/// line for real and warns again, so every such warning printed twice. The
/// sort's copy is the one that belongs. Thread-local, because the checks tidy
/// on the rayon pool while nothing else runs; restores the previous state, so
/// nesting is harmless.
pub(crate) struct SuppressWarnings(bool);

impl SuppressWarnings {
    pub(crate) fn new() -> Self {
        Self(WARNINGS_SUPPRESSED.with(|s| s.replace(true)))
    }
}

impl Drop for SuppressWarnings {
    fn drop(&mut self) {
        WARNINGS_SUPPRESSED.with(|s| s.set(self.0));
    }
}

/// Set by `--benchmark` once its warm-up run has printed each warning, so the
/// timed runs neither repeat them nor spend time writing them. Global rather
/// than thread-local: the sort runs on rayon's workers.
static WARNINGS_MUTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Write warning to buffer (if file output) or stderr
pub(crate) fn write_warning(message: &str) {
    if WARNINGS_SUPPRESSED.with(|s| s.get()) || WARNINGS_MUTED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if !WARNING_TO_FILE.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!("{}", message);
        return;
    }
    if let Ok(mut buffer) = WARNING_BUFFER.lock() {
        buffer.push(message.to_string());
    }
}

/// Flush buffered warnings to file
pub(crate) fn flush_warnings() {
    // Clone the output path and take the warnings out of the mutex so we don't hold locks during I/O.
    let path = {
        let Ok(guard) = WARNING_OUTPUT.lock() else { return };
        let Some(ref path) = *guard else { return };
        path.clone()
    };

    let warnings = {
        let Ok(mut buffer) = WARNING_BUFFER.lock() else { return };
        if buffer.is_empty() { return; }
        std::mem::take(&mut *buffer)
    };
    
    use std::io::{BufWriter, Write};
    if let Ok(file) = fop_sort::create_file_no_follow(&path) {
        let mut writer = BufWriter::new(file);
        for msg in warnings {
            let _ = write!(writer, "{}\n", msg);
        }
    }
}

use rayon::prelude::*;
use regex::Regex;
use walkdir::{DirEntry, WalkDir};

use fop_git::{
    build_base_command, check_repo_changes, commit_changes, create_pull_request, get_added_lines,
    git_available, get_remote_name, check_banned_domains, RepoDefinition, REPO_TYPES,
};
use fop_sort::{fop_sort, SortConfig, TRACK_CHANGES};

// FOP version number
const VERSION: &str = env!("CARGO_PKG_VERSION");

// =============================================================================
// Command Line Arguments
// =============================================================================

#[derive(Debug, Clone)]
struct Args {
    /// Directories to process
    directories: Vec<PathBuf>,
    /// Skip repository commit (just sort)
    no_commit: bool,
    /// Skip uBO to ABP option conversion
    no_ubo_convert: bool,
    /// Skip commit message format validation
    no_msg_check: bool,
    /// Disable IGNORE_FILES and IGNORE_DIRS checks
    disable_ignored: bool,
    /// Skip sorting (only combine rules)
    no_sort: bool,
    /// Use alternative sorting (sort by selector for all rule types)
    alt_sort: bool,
    /// Convert ABP extended selectors to uBO format
    abp_convert: bool,
    /// Promote a `:has-text()` rule's separator to AdGuard's `#?#` / `#@?#`
    adguard_convert: bool,
    /// Convert trusted scriptlets to non-trusted when value is safe
    convert_trusted: bool,
    /// Parse AdGuard extended CSS selectors (#$?# and #@$?#)
    parse_adguard: bool,
    /// Files to parse as AdGuard extended CSS (comma-separated)
    parse_adguard_files: Vec<String>,
    /// Sort localhost/hosts file entries (0.0.0.0/127.0.0.1)
    localhost: bool,
    /// Disable colored output
    no_color: bool,
    /// Additional files to ignore (comma-separated, supports partial names)
    ignore_files: Vec<String>,
    /// Additional directories to ignore (comma-separated, supports partial names)
    ignore_dirs: Vec<String>,
    /// Only process these files, ignore all others (comma-separated)
    ignore_all_but: Vec<String>,
    /// Disable large change warning prompt
    no_large_warning: bool,
    /// File extensions to process (default: .txt)
    file_extensions: Vec<String>,
    /// Comment line prefixes (default: !)
    comment_chars: Vec<String>,
    /// Create backup of files before modifying
    backup: bool,
    /// Keep empty lines in output
    keep_empty_lines: bool,
    /// Don't mention rules whose domain has no dot
    ignore_dot_domains: bool,
    /// Output warnings to file instead of stderr
    warning_output: Option<PathBuf>,
    /// Create PR branch instead of committing to master (optional: PR title)
    create_pr: Option<String>,
    /// Fix cosmetic typos in all processed files
    fix_typos: bool,
    /// Keep rules under the three-character floor instead of dropping them
    ignore_line_minimum: bool,
    /// Base branch for PR (default: auto-detect main/master)
    git_pr_branch: Option<String>,
    /// Include rule changes in PR body
    pr_show_changes: bool,
    /// Path to banned domain list file
    check_banned_list: Option<PathBuf>,
    /// Auto-remove banned domains and commit
    auto_banned_remove: bool,
    /// Check typos in git additions before commit
    fix_typos_on_add: bool,
    /// Check newly added lines for rules that cannot work
    check_rules_on_add: bool,
    /// Delete the flagged lines instead of only reporting them
    remove_bad_rules: bool,
    /// Users allowed to push directly (bypass create-pr)
    direct_push_users: Vec<String>,
    /// Auto-fix without prompting (use with --fix-typos or --fix-typos-on-add)
    auto_fix: bool,
    /// Output changes as diff file (no actual changes made)
    output_diff: Option<PathBuf>,  // Combined mode: single file
    /// Output individual .diff files alongside source files
    output_diff_individual: bool,
    /// Suppress most output (for CI)
    quiet: bool,
    /// Suppress directory listing only
    limited_quiet: bool,
    /// Output changed files with --changed suffix (no overwrite)
    output_changed: bool,
    /// Process a single file instead of directory
    check_file: Option<PathBuf>,
    /// Git commit message (skip interactive prompt)
    git_message: Option<String>,
    /// Only sort files changed according to git
    only_sort_changed: bool,
    /// Auto rebase and retry if push fails
    rebase_on_fail: bool,
    /// Worker threads for the rayon pool. None means size it automatically.
    threads: Option<usize>,
    /// Mask URLs in commit messages: 1=`[.]`, 2=`(.)`, 3=` ` (space)
    commit_mask: Option<u8>,
    /// If non-empty, only apply commit_mask when current git user.name is in this list (lowercased).
    commit_mask_users: Vec<String>,
    /// Also mask bare hostnames (no http/https scheme). Off by default — risks false positives.
    commit_mask_bare: bool,
    /// Additional apex hosts exempt from --commit-mask (apex match + dot-boundary subdomain).
    commit_mask_exempt_hosts: Vec<String>,
    /// Override the commit URL template (placeholders: {base}, {sha}). Default: {base}/commit/{sha}.
    commit_url_template: Option<String>,
    /// CI mode - exit with error code on failures
    ci: bool,
    /// Show applied configuration
    show_config: bool,
    /// Files to sort as localhost/hosts format (comma-separated)
    localhost_files: Vec<String>,
    /// Predefined commit message history for arrow key selection
    history: Vec<String>,
    /// Show help
    help: bool,
    /// Show version
    version: bool,
    /// Update timestamp in file header
    add_timestamp: Vec<String>,
    /// Add/update checksum for specific files
    add_checksum: Vec<String>,
    /// Validate checksum for specific files
    validate_checksum: Vec<String>,
    /// Validate and fix checksum for specific files
    validate_checksum_and_fix: Vec<String>,
    /// Custom git binary path
    git_binary: Option<String>,
    /// Benchmark mode - time processing and report metrics
    benchmark: bool,
    /// Timed benchmark runs, after one untimed warm-up
    benchmark_runs: usize,
    /// Per-file configuration overrides from [filename] sections in .fopconfig
    file_overrides: ahash::AHashMap<String, FileOverrides>,
}

/// Per-file configuration overrides from [filename] sections
#[derive(Debug, Clone, Default)]
struct FileOverrides {
    no_sort: Option<bool>,
    alt_sort: Option<bool>,
    parse_adguard: Option<bool>,
    localhost: Option<bool>,
    add_checksum: Option<bool>,
    add_timestamp: Option<bool>,
    no_ubo_convert: Option<bool>,
    abp_convert: Option<bool>,
    adguard_convert: Option<bool>,
    convert_trusted: Option<bool>,
    keep_empty_lines: Option<bool>,
    ignore_dot_domains: Option<bool>,
    fix_typos: Option<bool>,
    ignore_line_minimum: Option<bool>,
}

impl FileOverrides {
    /// Apply per-file overrides to a SortConfig
    fn apply_to(&self, config: &mut SortConfig) {
        if let Some(v) = self.no_sort { config.no_sort = v; }
        if let Some(v) = self.alt_sort { config.alt_sort = v; }
        if let Some(v) = self.parse_adguard { config.parse_adguard = v; }
        if let Some(v) = self.localhost { config.localhost = v; }
        if let Some(v) = self.keep_empty_lines { config.keep_empty_lines = v; }
        if let Some(v) = self.ignore_dot_domains { config.ignore_dot_domains = v; }
        if let Some(v) = self.fix_typos { config.fix_typos = v; }
        if let Some(v) = self.ignore_line_minimum { config.ignore_line_minimum = v; }
        if let Some(v) = self.abp_convert { config.abp_convert = v; }
        if let Some(v) = self.adguard_convert { config.adguard_convert = v; }
        if let Some(v) = self.convert_trusted { config.convert_trusted = v; }
        if let Some(v) = self.no_ubo_convert { config.convert_ubo = !v; }
        if let Some(true) = self.add_timestamp { config.add_timestamp = true; }
    }
}

/// Apply a key=value pair to a FileOverrides entry
fn apply_file_override(entry: &mut FileOverrides, key: &str, value: &str) {
    let b = value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes");
    match key {
        "no-sort" => entry.no_sort = Some(b),
        "alt-sort" => entry.alt_sort = Some(b),
        "parse-adguard" => entry.parse_adguard = Some(b),
        "localhost" => entry.localhost = Some(b),
        "add-checksum" => entry.add_checksum = Some(b),
        "add-timestamp" => entry.add_timestamp = Some(b),
        "no-ubo-convert" => entry.no_ubo_convert = Some(b),
        "abp-convert" => entry.abp_convert = Some(b),
        "adguard-convert" => entry.adguard_convert = Some(b),
        "convert-trusted" => entry.convert_trusted = Some(b),
        "keep-empty-lines" => entry.keep_empty_lines = Some(b),
        "ignore-dot-domains" => entry.ignore_dot_domains = Some(b),
        "fix-typos" => entry.fix_typos = Some(b),
        "ignore-line-minimum" => entry.ignore_line_minimum = Some(b),
        _ => {}
    }
}

/// Load configuration from .fopconfig file
fn load_config(custom_path: Option<&PathBuf>) -> (HashMap<String, String>, ahash::AHashMap<String, FileOverrides>, Option<PathBuf>) {
    // pre-allocated config settings
    let mut config = HashMap::with_capacity(28);
    let mut file_overrides: ahash::AHashMap<String, FileOverrides> = ahash::AHashMap::new();

    // If custom path provided, use that only
    let config_path: Option<PathBuf> = if let Some(path) = custom_path {
        if path.exists() {
            Some(path.clone())
        } else {
            eprintln!("Warning: Config file not found: {}", path.display());
            None
        }
    } else {
        // Try ./.fopconfig first, then ~/.fopconfig
        let config_paths = [
            PathBuf::from(".fopconfig"),
            home_dir()
                .map(|h| h.join(".fopconfig"))
                .unwrap_or_default(),
        ];
        config_paths.into_iter().find(|p| p.exists())
    };

    if let Some(path) = config_path.as_ref() {
        if let Ok(content) = fs::read_to_string(path) {
            let mut current_section: Option<String> = None;
            for line in content.lines() {
                let line = line.trim();
                // Skip comments and empty lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                // Detect [filename] section header
                if line.starts_with('[') && line.ends_with(']') {
                    current_section = Some(line[1..line.len() - 1].trim().to_string());
                    continue;
                }
                // Parse key = value
                if let Some(eq_pos) = line.find('=') {
                    let key = line[..eq_pos].trim();
                    let value = line[eq_pos + 1..].trim();
                    if let Some(ref section) = current_section {
                        let entry = file_overrides.entry(section.clone()).or_default();
                        apply_file_override(entry, key, value);
                    } else {
                        config.insert(key.to_string(), value.to_string());
                    }
                }
            }
        }
    }

    (config, file_overrides, config_path)
}

/// Parse boolean value from config
fn parse_bool(config: &HashMap<String, String>, key: &str, default: bool) -> bool {
    config
        .get(key)
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(default)
}

/// Parse string list from config (comma-separated)
fn parse_list(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    config
        .get(key)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Normalize extension to exclude leading dot (for path.extension() comparison)
fn normalize_extension(ext: &str) -> String {
    ext.trim_start_matches('.').to_string()
}

/// Parse file extensions from config (comma-separated), default to txt
fn parse_extensions(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    config
        .get(key)
        .map(|v| {
            v.split(',')
                .map(|s| normalize_extension(s.trim()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_else(|| vec!["txt".to_string()])
}

/// Parse comment characters from config (comma-separated), default to !
fn parse_comment_chars(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    config
        .get(key)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_else(|| vec!["!".to_string()])
}

impl Args {
    fn parse() -> (Self, Option<String>) {
        // Collect args once so we don't re-iterate env::args() multiple times.
        let argv: Vec<String> = env::args().skip(1).collect();

        // First pass: look for --ignore-config and --config-file arguments
        let ignore_config = argv.iter().any(|arg| arg == "--ignore-config");

        let mut config_file: Option<PathBuf> = None;
        if !ignore_config {
            for arg in &argv {
                if arg.starts_with("--config-file=") {
                    let path = arg.trim_start_matches("--config-file=");
                    config_file = Some(PathBuf::from(path));
                    break;
                }
            }
        }

        // Load config file and track path
        let (config, file_overrides, found_config_path) = if ignore_config {
            (HashMap::new(), ahash::AHashMap::new(), None)
        } else {
            load_config(config_file.as_ref())
        };
        // Store for --show-config
        let config_path_str = found_config_path.as_ref().map(|p| p.display().to_string());

        // Start with config values (or defaults)
        let mut args = Args {
            directories: Vec::new(),
            no_commit: parse_bool(&config, "no-commit", false),
            no_ubo_convert: parse_bool(&config, "no-ubo-convert", false),
            no_msg_check: parse_bool(&config, "no-msg-check", false),
            disable_ignored: parse_bool(&config, "disable-ignored", false),
            abp_convert: parse_bool(&config, "abp-convert", false),
            adguard_convert: parse_bool(&config, "adguard-convert", false),
            convert_trusted: parse_bool(&config, "convert-trusted", false),
            no_sort: parse_bool(&config, "no-sort", false),
            alt_sort: parse_bool(&config, "alt-sort", false),
            parse_adguard: parse_bool(&config, "parse-adguard", false),
            parse_adguard_files: parse_list(&config, "parse-adguard-files"),
            localhost: parse_bool(&config, "localhost", false),
            localhost_files: parse_list(&config, "localhost-files"),
            no_color: parse_bool(&config, "no-color", false),
            ignore_files: parse_list(&config, "ignorefiles"),
            ignore_dirs: parse_list(&config, "ignoredirs"),
            ignore_all_but: parse_list(&config, "ignore-all-but"),
            git_message: None,
            show_config: false,
            no_large_warning: parse_bool(&config, "no-large-warning", false),
            file_extensions: parse_extensions(&config, "file-extensions"),
            comment_chars: parse_comment_chars(&config, "comments"),
            backup: parse_bool(&config, "backup", false),
            keep_empty_lines: parse_bool(&config, "keep-empty-lines", false),
            ignore_dot_domains: parse_bool(&config, "ignore-dot-domains", false),
            warning_output: config.get("warning-output").and_then(|v| non_empty_path(v)),
            create_pr: config.get("create-pr").and_then(|v| {
                match v.to_lowercase().as_str() {
                    "" | "true" | "yes" | "1" => Some(String::new()), // Enable with prompt
                    "false" | "no" | "0" => None,                     // Disable
                    _ => Some(v.clone()),                             // Use as title
                }
            }),
            git_pr_branch: config.get("git-pr-branch").cloned(),
            pr_show_changes: parse_bool(&config, "pr-show-changes", false),
            check_banned_list: config.get("check-banned-list").and_then(|v| non_empty_path(v)),
            auto_banned_remove: parse_bool(&config, "auto-banned-remove", false),
            fix_typos: parse_bool(&config, "fix-typos", false),
            ignore_line_minimum: parse_bool(&config, "ignore-line-minimum", false),
            fix_typos_on_add: parse_bool(&config, "fix-typos-on-add", false),
            check_rules_on_add: parse_bool(&config, "check-rules-on-add", false),
            // `check_rules_on_add` is forced on below when this is set: the
            // CLI arm couples them deliberately, and leaving the config path
            // uncoupled makes `remove-bad-rules = true` alone silently do
            // nothing -- the exact trap that coupling avoids.
            remove_bad_rules: parse_bool(&config, "remove-bad-rules", false),
            direct_push_users: config.get("direct-push-users")
                .map(|s| s.split(',').map(|u| u.trim().to_lowercase()).collect())
                .unwrap_or_default(),
            quiet: parse_bool(&config, "quiet", false),
            limited_quiet: parse_bool(&config, "limited-quiet", false),
            auto_fix: parse_bool(&config, "auto-fix", false),
            output_diff: config.get("output-diff").and_then(|v| non_empty_path(v)),
            output_diff_individual: false,
            check_file: None,
            output_changed: false,
            only_sort_changed: parse_bool(&config, "only-sort-changed", false),
            rebase_on_fail: parse_bool(&config, "rebase-on-fail", true),
            // Anything not a whole number >= 1 is ignored rather than fatal:
            // a malformed config line should not stop a sort.
            threads: config
                .get("threads")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|&n| n >= 1),
            commit_mask: config.get("commit-mask")
                .and_then(|s| s.trim().parse::<u8>().ok()),
            // `parse_list` already trims and drops empty entries — important
            // here, because a bare `commit-mask-users =` line would otherwise
            // yield `[""]`, a non-empty allowlist that nothing can match, and
            // silently disable masking entirely.
            commit_mask_users: parse_list(&config, "commit-mask-users")
                .into_iter()
                .map(|u| u.to_lowercase())
                .collect(),
            commit_mask_bare: parse_bool(&config, "commit-mask-bare", false),
            commit_mask_exempt_hosts: parse_list(&config, "commit-mask-exempt-hosts")
                .into_iter()
                .map(|h| h.to_lowercase())
                .collect(),
            commit_url_template: config.get("commit-url-template")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            ci: parse_bool(&config, "ci", false),
            history: config.get("history")
                .map(|s| s.split(',')
                    .map(|item| item.trim().trim_matches('"').to_string())
                    .collect())
                .unwrap_or_default(),
            help: false,
            version: false,
            add_timestamp: parse_list(&config, "add-timestamp"),
            validate_checksum: Vec::new(),
            validate_checksum_and_fix: Vec::new(),
            add_checksum: config.get("add-checksum")
                .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
                .unwrap_or_default(),
            git_binary: config.get("git-binary").cloned(),
            benchmark: false,
            benchmark_runs: 5,
            file_overrides,
        };

        // What a repository's own .fopconfig chose, to be judged once the
        // command line has had its say (see restrict_repo_config)
        let repo_config = (config_file.is_none()
            && found_config_path.as_deref() == Some(Path::new(".fopconfig"))
            && !cwd_is_home())
            .then(|| (args.git_binary.clone(), args.warning_output.clone(), args.output_diff.clone()));

        // Command line args override config
        for arg in argv {
            match arg.as_str() {
                "-h" | "--help" => args.help = true,
                "-V" | "--version" => args.version = true,
                "-n" | "--no-commit" | "--just-sort" | "--justsort" => args.no_commit = true,
                "--no-ubo-convert" => args.no_ubo_convert = true,
                "--no-msg-check" => args.no_msg_check = true,
                "--disable-ignored" => args.disable_ignored = true,
                "--no-sort" => args.no_sort = true,
                "--alt-sort" => args.alt_sort = true,
                "--abp-convert" => args.abp_convert = true,
                "--adguard-convert" => args.adguard_convert = true,
                "--convert-trusted" => args.convert_trusted = true,
                "--parse-adguard" => args.parse_adguard = true,
                _ if arg.starts_with("--parse-adguard=") => {
                    args.parse_adguard_files = arg.trim_start_matches("--parse-adguard=")
                        .split(',').map(|s| s.trim().to_string()).collect();
                }
                "--localhost" => args.localhost = true,
                _ if arg.starts_with("--localhost-files=") => {
                    args.localhost_files = arg.trim_start_matches("--localhost-files=")
                        .split(',').map(|s| s.trim().to_string()).collect();
                }
                "--no-color" => args.no_color = true,
                "--no-large-warning" => args.no_large_warning = true,
                "--show-config" => args.show_config = true,
                "--only-sort-changed" => args.only_sort_changed = true,
                "--rebase-on-fail" => args.rebase_on_fail = true,
                "--no-rebase-on-fail" => args.rebase_on_fail = false,
                _ if arg.starts_with("--threads=") => {
                    let val = arg.trim_start_matches("--threads=").trim();
                    match val.parse::<usize>() {
                        Ok(n) if n >= 1 => args.threads = Some(n.min(MAX_THREADS)),
                        Ok(_) => {
                            eprintln!("Error: --threads must be at least 1 (got '{}')", val);
                            std::process::exit(2);
                        }
                        Err(_) => {
                            eprintln!("Error: --threads must be a whole number (got '{}')", val);
                            std::process::exit(2);
                        }
                    }
                }
                _ if arg.starts_with("--commit-mask=") => {
                    let val = arg.trim_start_matches("--commit-mask=");
                    match val.parse::<u8>() {
                        Ok(n) => args.commit_mask = Some(n),
                        Err(_) => {
                            eprintln!("Error: --commit-mask must be a number (got '{}')", val);
                            std::process::exit(2);
                        }
                    }
                }
                // Turns masking off even when .fopconfig sets a level.
                // `--commit-mask=0` can't do this: 0 falls through to level 1
                // by design, so without this a config-set level was unreachable
                // from the command line.
                "--no-commit-mask" => args.commit_mask = None,
                _ if arg.starts_with("--commit-mask-users=") => {
                    let users = arg.trim_start_matches("--commit-mask-users=");
                    args.commit_mask_users = users.split(',')
                        .map(|u| u.trim().to_lowercase())
                        .filter(|u| !u.is_empty())
                        .collect();
                }
                "--commit-mask-bare" => args.commit_mask_bare = true,
                "--no-commit-mask-bare" => args.commit_mask_bare = false,
                _ if arg.starts_with("--commit-mask-exempt-hosts=") => {
                    let hosts = arg.trim_start_matches("--commit-mask-exempt-hosts=");
                    args.commit_mask_exempt_hosts = hosts.split(',')
                        .map(|h| h.trim().to_lowercase())
                        .filter(|h| !h.is_empty())
                        .collect();
                }
                _ if arg.starts_with("--commit-url-template=") => {
                    let tmpl = arg.trim_start_matches("--commit-url-template=").trim();
                    args.commit_url_template = if tmpl.is_empty() { None } else { Some(tmpl.to_string()) };
                }
                "--pr-show-changes" => args.pr_show_changes = true,
                _ if arg.starts_with("--check-banned-list=") => {
                    args.check_banned_list = Some(cli_path("--check-banned-list", arg.trim_start_matches("--check-banned-list=")));
                }
                "--auto-banned-remove" => args.auto_banned_remove = true,
                _ if arg.starts_with("--ignorefiles=") => {
                    let files = arg.trim_start_matches("--ignorefiles=");
                    args.ignore_files = files.split(',').map(|s| s.trim().to_string()).collect();
                }
                _ if arg.starts_with("--ignore-all-but=") => {
                    let files = arg.trim_start_matches("--ignore-all-but=");
                    args.ignore_all_but = files.split(',').map(|s| s.trim().to_string()).collect();
                }
                _ if arg.starts_with("--file-extensions=") => {
                    args.file_extensions = arg
                        .trim_start_matches("--file-extensions=")
                        .split(',')
                        .map(|s| normalize_extension(s.trim()))
                        .collect();
                }
                _ if arg.starts_with("--comments=") => {
                    args.comment_chars = arg
                        .trim_start_matches("--comments=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "--backup" => args.backup = true,
                "--keep-empty-lines" => args.keep_empty_lines = true,
                "--ignore-dot-domains" => args.ignore_dot_domains = true,
                _ if arg.starts_with("--warning-output=") => {
                    args.warning_output = Some(cli_path("--warning-output", arg.trim_start_matches("--warning-output=")));
                }
                _ if arg.starts_with("--config-file=") => {
                    // Already handled in first pass
                }
                _ if arg.starts_with("--ignoredirs=") => {
                    args.ignore_dirs = arg
                        .trim_start_matches("--ignoredirs=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                "--create-pr" => args.create_pr = Some(String::new()),
                _ if arg.starts_with("--create-pr=") => {
                    args.create_pr = Some(arg.trim_start_matches("--create-pr=").to_string());
                }
                _ if arg.starts_with("--git-pr-branch=") => {
                    args.git_pr_branch =
                        Some(arg.trim_start_matches("--git-pr-branch=").to_string());
                }
                "--fix-typos" => args.fix_typos = true,
                "--ignore-line-minimum" => args.ignore_line_minimum = true,
                "--fix-typos-on-add" => args.fix_typos_on_add = true,
                "--check-rules-on-add" => args.check_rules_on_add = true,
                // Removing implies checking: the flag is useless alone, and
                // requiring both would be a trap that silently does nothing.
                "--remove-bad-rules" => {
                    args.remove_bad_rules = true;
                    args.check_rules_on_add = true;
                }
                "--auto-fix" => args.auto_fix = true,
                _ if arg.starts_with("--add-timestamp=") => {
                    args.add_timestamp = arg.trim_start_matches("--add-timestamp=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ if arg.starts_with("--add-checksum=") => {
                    args.add_checksum = arg.trim_start_matches("--add-checksum=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                "--ignore-config" => {} // Already handled early
                _ if arg.starts_with("--check-file=") => {
                    args.check_file = Some(cli_path("--check-file", arg.trim_start_matches("--check-file=")));
                }
                "--quiet" | "-q" => args.quiet = true,
                "--limited-quiet" => args.limited_quiet = true,
                "--ci" => args.ci = true,
                "--benchmark" => args.benchmark = true,
                _ if arg.starts_with("--benchmark=") => {
                    let val = arg.trim_start_matches("--benchmark=").trim();
                    match val.parse::<usize>() {
                        Ok(n) if n >= 1 => {
                            args.benchmark = true;
                            args.benchmark_runs = n;
                        }
                        Ok(_) => {
                            eprintln!("Error: --benchmark must be at least 1 (got '{}')", val);
                            std::process::exit(2);
                        }
                        Err(_) => {
                            eprintln!("Error: --benchmark must be a whole number (got '{}')", val);
                            std::process::exit(2);
                        }
                    }
                }
                _ if arg.starts_with("--history=") => {
                    args.history = arg.trim_start_matches("--history=")
                        .split(',')
                        .map(|s| s.trim_matches('"').to_string())
                        .collect();
                }
                "--output-diff" => {
                    // Individual mode: create .diff file for each source file
                    args.output_diff_individual = true;
                }
                "--output" => {
                    args.output_changed = true;
                }
                _ if arg.starts_with("--output-diff=") => {
                    args.output_diff =
                        Some(cli_path("--output-diff", arg.trim_start_matches("--output-diff=")));
                }
                _ if arg.starts_with("--git-message=") => {
                    args.git_message = Some(arg.trim_start_matches("--git-message=").to_string());
                }
                _ if arg.starts_with("--validate-checksum=") => {
                    args.validate_checksum = arg.trim_start_matches("--validate-checksum=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ if arg.starts_with("--validate-checksum-and-fix=") => {
                    args.validate_checksum_and_fix = arg.trim_start_matches("--validate-checksum-and-fix=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ if arg.starts_with("--git-binary=") => {
                    args.git_binary = Some(arg.trim_start_matches("--git-binary=").to_string());
                }
                _ if arg.starts_with('-') => {
                    eprintln!("Unknown option: {}", arg);
                    eprintln!("Use --help for usage information");
                    std::process::exit(1);
                }
                _ => args.directories.push(PathBuf::from(arg)),
            }
        }

        // Warn about incompatible options
        if (args.output_diff.is_some() || args.output_diff_individual) && args.create_pr.is_some() {
            // Silently disable create-pr when using output-diff (common when config has create-pr)
            args.create_pr = None;
        }
        if (args.output_diff.is_some() || args.output_diff_individual) && args.git_message.is_some() {
            // Silently disable git-message when using output-diff
            args.git_message = None;
        }
        if args.output_diff.is_some() && args.output_diff_individual {
            eprintln!("Warning: --output-diff and --output-diff=<file> are mutually exclusive");
            eprintln!("Using individual mode (--output-diff)");
            args.output_diff = None;
        }
        if args.no_commit && args.create_pr.is_some() {
            eprintln!("Warning: --no-commit and --create-pr are incompatible");
            args.create_pr = None;
        }
        if args.output_changed && (args.output_diff.is_some() || args.output_diff_individual) {
            eprintln!("Warning: --output and --output-diff are mutually exclusive");
            eprintln!("Using --output");
            args.output_diff = None;
            args.output_diff_individual = false;
        }
        if args.output_changed && args.create_pr.is_some() {
            eprintln!("Warning: --output and --create-pr are incompatible");
            args.create_pr = None;
        }
        if args.no_commit && args.git_message.is_some() {
            eprintln!("Warning: --no-commit and --git-message are incompatible");
        }

        // Removing implies checking, however the flag arrived.
        if args.remove_bad_rules {
            args.check_rules_on_add = true;
        }

        if let Some(from_config) = repo_config {
            if let Err(e) = restrict_repo_config(&mut args.git_binary, &mut args.warning_output, &args.output_diff, from_config) {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        }

        (args, config_path_str)
    }

    fn print_help() {
        println!("FOP - Filter Orderer and Preener v{}", VERSION);
        println!();
        println!("USAGE:");
        println!("    fop [OPTIONS] [DIRECTORIES]...");
        println!();
        println!("ARGUMENTS:");
        println!("    [DIRECTORIES]...    Directories to process (default: current directory)");
        println!();
        println!("OPTIONS:");
        println!("    -n, --no-commit     Just sort files, skip Git commit prompts");
        println!("        --just-sort     Alias for --no-commit");
        println!("        --no-ubo-convert  Skip uBO to ABP option conversion");
        println!("        --no-msg-check  Skip commit message format validation (M:/A:/P:)");
        println!("        --disable-ignored  Process all files (ignore IGNORE_FILES/IGNORE_DIRS)");
        println!("        --no-sort       Skip sorting (only tidy and combine rules)");
        println!("        --alt-sort      Alternative sorting (by selector for all rule types)");
        println!("        --parse-adguard Parse AdGuard extended CSS (#$?#, #@$?#, $$, $@$)");
        println!("        --parse-adguard=  Files to parse as AdGuard extended CSS (comma-separated)");
        println!("        --localhost     Sort hosts file entries (0.0.0.0/127.0.0.1 domain)");
        println!("        --localhost-files=  Files to sort as localhost format (comma-separated)");
        println!("        --no-color      Disable colored output");
        println!("        --commit-mask=N Mask URLs in commit messages (1=[.], 2=(.), 3=space, 4=preserve subdomain dot, 5=Unicode lookalike)");
        println!("        --no-commit-mask    Disable URL masking even if .fopconfig sets commit-mask");
        println!("        --commit-mask-users=u1,u2  Restrict --commit-mask to these git user.name values (lowercased)");
        println!("        --commit-mask-bare    Also mask bare hostnames (no http/https). Risks FP on filenames.");
        println!("        --commit-mask-exempt-hosts=h1,h2  Additional apex hosts exempt from masking (e.g. self-hosted Gitea/Forgejo)");
        println!("        --commit-url-template=TMPL  Override 'Commit successful' URL template ({{base}}, {{sha}}); default {{base}}/commit/{{sha}}");
        println!("        --rebase-on-fail    Auto 'git pull --rebase --autostash' and retry a failed push (default: on)");
        println!("        --no-rebase-on-fail Don't auto-rebase; print the suggested command and stop");
        println!("        --no-large-warning  Disable large change warning prompt");
        println!("        --ignorefiles=  Additional files to ignore (comma-separated, partial names)");
        println!("        --abp-convert          Convert :-abp-has/:-abp-contains to :has/:has-text");
        println!("        --adguard-convert      Promote :has-text() separators to AdGuard's #?# / #@?#");
        println!("        --convert-trusted      Convert trusted scriptlets to non-trusted when value is safe");
        println!("        --ignoredirs=   Additional directories to ignore (comma-separated, partial names)");
        println!("        --ignore-all-but=   Only process these files, ignore all others (comma-separated)");
        println!("        --config-file=  Custom config file path");
        println!("        --file-extensions=  File extensions to process (default: .txt)");
        println!("        --comments=     Comment line prefixes (default: !)");
        println!("        --backup        Create .backup files before modifying");
        println!("        --keep-empty-lines  Keep empty lines in output");
        println!("        --ignore-dot-domains  Don't mention rules whose domain has no dot");
        println!("        --warning-output=   Output warnings to file instead of stderr");
        println!("        --git-message=  Git commit message (skip interactive prompt)");
        println!("        --create-pr[=TITLE]  Create PR branch instead of committing to master");
        println!("        --git-pr-branch=NAME   Base branch for PR (default: main/master)");
        println!("        --fix-typos      Fix cosmetic rule typos in all files");
        println!("        --fix-typos-on-add   Check cosmetic rule typos in git additions");
        println!("        --check-rules-on-add  Check git additions for rules that cannot work");
        println!("        --remove-bad-rules    Delete defective lines instead of reporting them (advice is kept)");
        println!("        --ignore-line-minimum  Keep rules under 3 chars instead of dropping them");
        println!("        --auto-fix           Auto-fix typos without prompting");
        println!("        --threads=N         Worker threads (default: cores, capped at 8; overrides RAYON_NUM_THREADS)");
        println!("    -q, --quiet                Suppress most output (for CI)");
        println!("        --limited-quiet        Suppress directory listing only");
        println!("        --check-file=FILE      Process a single file");
        println!("        --output-diff=FILE     Output changes as diff (no files modified)");
        println!("        --output-diff          Output individual .diff files per source file");
        println!("        --output               Output changed files with --changed suffix");
        println!("        --ignore-config        Ignore .fopconfig file");
        println!("        --add-timestamp        Update 'Last modified/updated' timestamp in header");
        println!("        --add-timestamp=FILES  Add/update timestamp for specific files (comma-separated)");
        println!("        --add-checksum=FILES   Add/update checksum for specific files (comma-separated)");
        println!("        --validate-checksum=FILES  Validate checksum for specific files (exit 1 on failure)");
        println!("        --validate-checksum-and-fix=FILES  Validate and fix invalid checksums");
        println!("        --benchmark[=N] Time the sort: 1 warm-up, then N runs (default 5); no files changed");
        println!("        --show-config   Show applied configuration and exit");
        println!("        --git-binary=<path>    Path to git binary (default: git in PATH)");
        println!("    -h, --help          Show this help message");
        println!("    -V, --version       Show version number");
        println!();
        println!("EXAMPLES:");
        println!("    fop                          # Sort filters in current directory");
        println!("    fop /path/to/easylist        # Sort filters in specified directory");
        println!("    fop --no-commit .            # Sort without commit prompt");
        println!("    fop -n ~/easylist ~/fanboy   # Sort multiple directories, no commit");
        println!("    fop --ignorefiles=backup.txt,test.txt -n .");
        println!("                                 # Ignore specific files");
        println!("    fop --config-file=/path/to/.fopconfig -n .");
        println!("                                 # Use custom config file");
        println!("    fop --git-message=\"M: Fixed typo\" .");
        println!("                                 # Auto-commit with message");
        println!();
        println!("Config file (.fopconfig):");
        println!("    Place in current directory or home directory.");
        println!("    Command line arguments override config file settings.");
    }

    fn print_version() {
        println!("FOP version {}", VERSION);
    }
    fn print_config(&self, config_path: Option<&str>) {
        println!("FOP Configuration");
        println!("=================");
        println!();
        if let Some(path) = config_path {
            println!("Config file: {}", path);
        } else {
            println!("Config file: (none found, using defaults)");
        }
        println!();
        println!("Settings:");
        println!("  no-commit       = {}", self.no_commit);
        println!("  only-sort-changed = {}", self.only_sort_changed);
        println!("  rebase-on-fail  = {}", self.rebase_on_fail);
        let (workers, source) = resolve_workers(self.threads);
        println!("  threads         = {} ({})", workers, source);
        println!("  commit-mask     = {}", match self.commit_mask {
            Some(2) => "2 ((.))",
            Some(3) => "3 (space)",
            Some(4) => "4 (preserve subdomain dot)",
            Some(5) => "5 (Unicode \u{2024} lookalike)",
            Some(_) => "1 ([.])",
            None    => "off",
        });
        if !self.commit_mask_users.is_empty() {
            println!("  commit-mask-users = {}", self.commit_mask_users.join(","));
        }
        println!("  commit-mask-bare = {}", self.commit_mask_bare);
        if !self.commit_mask_exempt_hosts.is_empty() {
            println!("  commit-mask-exempt-hosts = {}", self.commit_mask_exempt_hosts.join(","));
        }
        if let Some(ref t) = self.commit_url_template {
            println!("  commit-url-template = {}", t);
        }
        println!("  ci              = {}", self.ci);
        println!("  pr-show-changes = {}", self.pr_show_changes);
        println!("  check-banned-list = {:?}", self.check_banned_list);
        println!("  no-ubo-convert  = {}", self.no_ubo_convert);
        println!("  no-msg-check    = {}", self.no_msg_check);
        println!("  disable-ignored = {}", self.disable_ignored);
        println!("  no-sort         = {}", self.no_sort);
        println!("  alt-sort        = {}", self.alt_sort);
        println!("  parse-adguard   = {}", self.parse_adguard);
        if self.parse_adguard_files.is_empty() {
            println!("  parse-adguard-files = (none)");
        } else {
            println!("  parse-adguard-files = {}", self.parse_adguard_files.join(","));
        }
        println!("  localhost       = {}", self.localhost);
        if self.localhost_files.is_empty() {
            println!("  localhost-files = (none)");
        } else {
            println!("  localhost-files = {}", self.localhost_files.join(","));
        }
        println!("  no-color        = {}", self.no_color);
        println!("  no-large-warning= {}", self.no_large_warning);
        println!();
        if self.ignore_files.is_empty() {
            println!("  ignorefiles     = (none)");
        } else {
            println!("  ignorefiles     = {}", self.ignore_files.join(","));
        }
        if self.ignore_dirs.is_empty() {
            println!("  ignoredirs      = (none)");
        } else {
            println!("  ignoredirs      = {}", self.ignore_dirs.join(","));
        }
        if self.ignore_all_but.is_empty() {
            println!("  ignore-all-but  = (none)");
        } else {
            println!("  ignore-all-but  = {}", self.ignore_all_but.join(","));
        }
        if self.file_extensions.is_empty()
            || (self.file_extensions.len() == 1 && self.file_extensions[0] == "txt")
        {
            println!("  file-extensions = txt (default)");
        } else {
            println!("  file-extensions = {}", self.file_extensions.join(","));
        }
        if self.comment_chars.len() == 1 && self.comment_chars[0] == "!" {
            println!("  comments        = ! (default)");
        } else {
            println!("  comments        = {}", self.comment_chars.join(","));
        }
        println!("  backup          = {}", self.backup);
        println!("  keep-empty-lines= {}", self.keep_empty_lines);
        println!("  ignore-dot-domains= {}", self.ignore_dot_domains);
        if let Some(ref path) = self.warning_output {
            println!("  warning-output  = {}", path.display());
        } else {
            println!("  warning-output  = (stderr)");
        }
        if let Some(ref title) = self.create_pr {
            println!("  create-pr       = {}", if title.is_empty() { "(prompt)" } else { title });
            if !self.direct_push_users.is_empty() {
                println!("  direct-push-users = {}", self.direct_push_users.join(","));
            }
        } else {
            println!("  create-pr       = false");
        }
        if !self.file_overrides.is_empty() {
            println!();
            println!("Per-file overrides:");
            for (file, overrides) in &self.file_overrides {
                println!("  [{}]", file);
                if let Some(v) = overrides.no_sort { println!("    no-sort = {}", v); }
                if let Some(v) = overrides.alt_sort { println!("    alt-sort = {}", v); }
                if let Some(v) = overrides.parse_adguard { println!("    parse-adguard = {}", v); }
                if let Some(v) = overrides.localhost { println!("    localhost = {}", v); }
                if let Some(v) = overrides.add_checksum { println!("    add-checksum = {}", v); }
                if let Some(v) = overrides.add_timestamp { println!("    add-timestamp = {}", v); }
                if let Some(v) = overrides.no_ubo_convert { println!("    no-ubo-convert = {}", v); }
                if let Some(v) = overrides.abp_convert { println!("    abp-convert = {}", v); }
                if let Some(v) = overrides.adguard_convert { println!("    adguard-convert = {}", v); }
                if let Some(v) = overrides.convert_trusted { println!("    convert-trusted = {}", v); }
                if let Some(v) = overrides.keep_empty_lines { println!("    keep-empty-lines = {}", v); }
                if let Some(v) = overrides.ignore_dot_domains { println!("    ignore-dot-domains = {}", v); }
                if let Some(v) = overrides.fix_typos { println!("    fix-typos = {}", v); }
                if let Some(v) = overrides.ignore_line_minimum { println!("    ignore-line-minimum = {}", v); }
            }
        }

        println!();
        print!("Press Enter to continue...");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
    }
}

// =============================================================================
// Regex Patterns (shared with fop_sort module)
// =============================================================================

/// Pattern for extracting domain from blocking filter options
pub(crate) static FILTER_DOMAIN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$(?:[^,]*,)*domain=([^,]+)").unwrap());

/// Pattern for extracting domain from element hiding rules  
pub(crate) static ELEMENT_DOMAIN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^([^/|@"!]*?)#[@?$%]?#"#).unwrap());

/// Pattern for extracting domain from AdGuard extended element rules
pub(crate) static ADGUARD_ELEMENT_DOMAIN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^([^/|@"!]*?)(#[@?$%]?#|#\$\?#|#@\$\?#|\$\$|\$@\$)"#).unwrap());

/// Pattern for AdGuard extended element matching (includes #$?# and #@$?#)
pub(crate) static ADGUARD_ELEMENT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^([^/|@"!]*?)(#[@?$%]?#|#@[$%?]#|#\$\?#|#@\$\?#|\$\$|\$@\$)(.+)$"#).unwrap());

/// Pattern for FOP element matching (no {} in selector)
pub(crate) static FOPPY_ELEMENT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^([^/|@"!]*?)(#[@?$%]?#|#@[$%?]#)([^{}]+)$"#).unwrap());

/// Pattern for FOP.py compatible sorting (only ## and #@#)
pub(crate) static FOPPY_ELEMENT_DOMAIN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^[^/|@"!]*?#@?#"#).unwrap());

/// Pattern for element hiding rules (standard, uBO, and AdGuard extended syntax)
pub(crate) static ELEMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^([^/|@"!]*?)(##|#@#|#\?#|#@\?#|#\$#|#@\$#|#%#|#@%#)(.+)$"#).unwrap()
});

/// Pattern for regex domain element hiding rules (uBO/AdGuard specific)
pub(crate) static REGEX_ELEMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(/[^#]+/)(##|#@#|#\?#|#@\?#|#\$#|#@\$#|#%#|#@%#)(.+)$"#).unwrap()
});

pub(crate) static OPTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.*[^\\]|)\$(~?[\w\-]+(?:=[^,\s]+)?(?:,~?[\w\-]+(?:=[^,\s]+)?)*)$").unwrap()
});

pub(crate) static PSEUDO_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(:[a-zA-Z\-]*[A-Z][a-zA-Z\-]*)").unwrap());

pub(crate) static REMOVAL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([>+~,@\s])(\*)([#.\[:])").expect("Invalid REMOVAL_PATTERN regex")
});

pub(crate) static ATTRIBUTE_VALUE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^([^'"\\]|\\.)*("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|\*"#).unwrap()
});

pub(crate) static TREE_SELECTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\\.|[^+>~ \t])\s*([+>~ \t])\s*(\D)").unwrap());

pub(crate) static UNICODE_SELECTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\[0-9a-fA-F]{1,6}\s[a-zA-Z]*[A-Z]").unwrap());

pub(crate) static DOMAIN_EXTRACT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\|*([^/\^\$]+)").unwrap());

// =============================================================================
// Constants
// =============================================================================

/// Files that should not be sorted
const IGNORE_FILES: &[&str] = &["test-files-to-ingore.txt"];

/// Directories to ignore
const IGNORE_DIRS: &[&str] = &["folders-to-ingore"];

/// Known Adblock Plus options (HashSet for O(1) lookup)
pub(crate) static KNOWN_OPTIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        // Standard ABP options
        "collapse",
        "csp",
        "csp=frame-src",
        "csp=img-src",
        "csp=media-src",
        "csp=script-src",
        "csp=worker-src",
        "document",
        "elemhide",
        "font",
        "genericblock",
        "generichide",
        "image",
        "match-case",
        "media",
        "object-subrequest",
        "object",
        "other",
        "ping",
        "popup",
        "script",
        "stylesheet",
        "subdocument",
        "third-party",
        "webrtc",
        "websocket",
        "xmlhttprequest",
        // uBO short options
        "xhr",
        "css",
        "1p",
        "3p",
        "frame",
        "doc",
        "ghide",
        "xml",
        "iframe",
        "first-party",
        "strict1p",
        "strict3p",
        "ehide",
        "shide",
        "specifichide",
        // uBO/ABP specific
        "all",
        "badfilter",
        "important",
        "popunder",
        "empty",
        "cname",
        "inline-script",
        "removeparam",
        "redirect-rule",
        "_____",
        "-----",
        // Adguard
        "network",
        "content",
        "extension",
        "jsinject",
        "stealth",
        "cookie",
        // ABP rewrite resources
        "rewrite=abp-resource:1x1-transparent-gif",
        "rewrite=abp-resource:2x2-transparent-png",
        "rewrite=abp-resource:32x32-transparent-png",
        "rewrite=abp-resource:3x2-transparent-png",
        "rewrite=abp-resource:blank-css",
        "rewrite=abp-resource:blank-html",
        "rewrite=abp-resource:blank-js",
        "rewrite=abp-resource:blank-mp3",
        "rewrite=abp-resource:blank-mp4",
        "rewrite=abp-resource:blank-text",
    ]
    .into_iter()
    .collect()
});

/// Option keys that take a `=value`. Kept separate from `KNOWN_OPTIONS`, whose
/// entries are matched whole: `csp` is a valid bare option *and* a valid
/// prefix, so the two sets deliberately overlap.
/// Options `KNOWN_OPTIONS` omitted. Harmless while an unknown option was only
/// a warning; with the addition checks it would delete a valid rule.
pub(crate) static EXTRA_KNOWN_OPTIONS: [&str; 8] = [
    "inline-font", "beacon", "mp4", "noop", "queryprune",
    // AdGuard's spelling of uBO's strict1p / strict3p.
    "strict-first-party", "strict-third-party",
    // A resource type in ABP and uBO: requests for a Web Bundle.
    "webbundle",
];

/// Modifiers valid bare only on an exception rule, where they switch off every
/// rule of that kind for the site: `@@||site^$removeheader`, `@@||site^$urlblock`.
/// On a blocking rule the bare word is missing its value, so there it stays
/// unknown. Missing entirely, these were flagged "unknown option" and deleted
/// under `--remove-bad-rules`.
pub(crate) static EXCEPTION_BARE_OPTIONS: [&str; 13] = [
    "urlblock", "removeheader", "replace", "redirect", "permissions",
    "urltransform", "uritransform", "urlskip", "hls", "jsonprune", "xmlprune",
    "referrerpolicy", "dnsrewrite",
];

pub(crate) static KNOWN_OPTION_PREFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "addheader", "app", "cookie", "csp", "denyallow", "domain", "from",
        "header", "hls", "ipaddress", "jsonprune", "method", "permissions",
        "reason", "redirect", "redirect-rule", "referrerpolicy", "removeheader",
        "removeparam", "replace", "requestheader", "responseheader", "rewrite",
        "sitekey", "stealth", "tag", "to", "top", "uritransform", "urlskip",
        "urltransform", "xmlprune",
        // AdGuard DNS filtering, and uBO's deprecated removeparam alias.
        "dnsrewrite", "dnstype", "client", "ctag", "queryprune",
    ]
    .into_iter()
    .collect()
});

/// Edit distance between `a` and `b`, or `None` once it provably exceeds
/// `max`.
///
/// Two rolling rows rather than a matrix, on the stack: option names are
/// short, and anything long enough to overflow the buffer is not a near-miss
/// for one. Rows are bailed out of as soon as every cell exceeds `max`, so a
/// distant candidate costs a fraction of the full computation.
fn edit_distance_within(a: &str, b: &str, max: usize) -> Option<usize> {
    // Rows are u8: no distance here can exceed CAP, and the narrower rows mean
    // an eighth of the stack traffic and a single cache line per row.
    const CAP: usize = 48;
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() >= CAP || b.len() >= CAP || a.len().abs_diff(b.len()) > max {
        return None;
    }
    let max = max as u8;
    let mut prev = [0u8; CAP];
    let mut curr = [0u8; CAP];
    for (j, slot) in prev.iter_mut().enumerate().take(b.len() + 1) {
        *slot = j as u8;
    }
    for i in 1..=a.len() {
        curr[0] = i as u8;
        let mut row_best = i as u8;
        for j in 1..=b.len() {
            let cost = u8::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            row_best = row_best.min(curr[j]);
        }
        if row_best > max {
            return None;
        }
        prev[..=b.len()].copy_from_slice(&curr[..=b.len()]);
    }
    (prev[b.len()] <= max).then_some(prev[b.len()] as usize)
}

/// The known option `unknown` was most likely meant to be.
///
/// Only consulted for options that already failed `is_known_option`, so the
/// ~100 candidate comparisons never touch a well-formed rule. Short names get
/// a tighter budget: at distance 2, `app` is as close to `all` as to anything.
pub(crate) fn suggest_option(unknown: &str) -> Option<&'static str> {
    let max = if unknown.len() <= 4 { 1 } else { 2 };
    let mut best: Option<(usize, &'static str)> = None;
    // Both sets are hashed with a randomised hasher, so iteration order varies
    // per process. Ties break on the name to keep the suggestion reproducible.
    for candidate in KNOWN_OPTIONS
        .iter()
        .chain(KNOWN_OPTION_PREFIXES.iter())
        .chain(EXTRA_KNOWN_OPTIONS.iter())
    {
        // A candidate equal to what was typed is no suggestion: a bare
        // `$requestheader` is wrong because it needs a value, and "did you
        // mean requestheader?" says nothing.
        if *candidate == unknown {
            continue;
        }
        if let Some(d) = edit_distance_within(unknown, candidate, max) {
            if best.is_none_or(|(bd, bn)| (d, *candidate) < (bd, bn)) {
                best = Some((d, candidate));
            }
        }
    }
    best.map(|(_, name)| name)
}

/// Whether `stripped` (a single option, `~` already removed) is one FOP knows.
///
/// One hash lookup for the whole-word forms and one more for the `key=value`
/// forms, rather than walking a chain of `starts_with` per option.
#[inline]
pub(crate) fn is_known_option(stripped: &str) -> bool {
    // AdGuard's noop modifier is a run of underscores of any length, used to
    // keep a long rule readable. There is nothing to look up, so it is
    // recognised by shape rather than by listing every length.
    if !stripped.is_empty() && stripped.bytes().all(|b| b == b'_') {
        return true;
    }
    KNOWN_OPTIONS.contains(stripped)
        || EXTRA_KNOWN_OPTIONS.contains(&stripped)
        || stripped
            .split_once('=')
            .is_some_and(|(key, _)| KNOWN_OPTION_PREFIXES.contains(key))
}

/// `is_known_option`, for a rule that may be an exception (`@@`).
#[inline]
pub(crate) fn is_known_option_in(stripped: &str, exception: bool) -> bool {
    is_known_option(stripped) || (exception && EXCEPTION_BARE_OPTIONS.contains(&stripped))
}

/// uBO to ABP option conversions
pub(crate) static UBO_CONVERSIONS: LazyLock<AHashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        [
            ("xhr", "xmlhttprequest"),
            ("~xhr", "~xmlhttprequest"),
            ("css", "stylesheet"),
            ("~css", "~stylesheet"),
            ("1p", "~third-party"),
            ("~1p", "third-party"),
            ("3p", "third-party"),
            ("~3p", "~third-party"),
            ("frame", "subdocument"),
            ("~frame", "~subdocument"),
            ("doc", "document"),
            ("ghide", "generichide"),
            ("xml", "xmlhttprequest"),
            ("~xml", "~xmlhttprequest"),
            ("iframe", "subdocument"),
            ("~iframe", "~subdocument"),
        ]
        .into_iter()
        .collect()
    });

// =============================================================================
// Main Processing
// =============================================================================

/// Check if filename matches any ignore pattern (exact or partial)
#[inline]
fn should_ignore_file(filename: &str, ignore_files: &[String]) -> bool {
    ignore_files
        .iter()
        .any(|pattern| filename == pattern || filename.contains(pattern))
}

/// Whether a path from a git diff names a file fop would sort.
///
/// The rule checks must only look at filter lists. Without this they run on
/// every added line in the repository, and any line carrying a `$` -- a shell
/// `$PATH`, a workflow's `$GITHUB_SHA` -- reads as a network rule with a bad
/// option. Mirrors the filter used to collect files for sorting, applied to
/// the diff's repo-relative path.
fn diff_path_is_filter_list(
    file: &str,
    file_extensions: &[String],
    ignore_files: &[String],
    ignore_dirs: &[String],
    ignore_all_but: &[String],
    disable_ignored: bool,
) -> bool {
    let path = Path::new(file);
    if should_ignore_dir(path, ignore_dirs) {
        return false;
    }
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    file_extensions.iter().any(|ext| ext == extension)
        && (disable_ignored || !IGNORE_FILES.contains(&filename))
        && !should_ignore_file(filename, ignore_files)
        // The sorter restricts itself to these; the diff must not reach past
        // the files fop was actually asked to touch.
        && (ignore_all_but.is_empty() || ignore_all_but.iter().any(|f| filename.contains(f)))
}

/// Check if directory path matches any ignore pattern
#[inline]
fn should_ignore_dir(path: &Path, ignore_dirs: &[String]) -> bool {
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            if ignore_dirs.iter().any(|p| name == p || name.contains(p)) {
                return true;
            }
        }
    }
    false
}

#[inline]
fn entry_is_dir(entry: &DirEntry) -> bool {
    let ft = entry.file_type();
    ft.is_dir() || (ft.is_symlink() && entry.path().is_dir())
}

#[inline]
fn entry_is_file(entry: &DirEntry, root: &Path) -> bool {
    let ft = entry.file_type();
    ft.is_file() || (ft.is_symlink() && link_in_tree(entry.path(), root))
}

/// Whether the symlink at `path` resolves to a file inside `root` (canonical).
/// A list that links out of the tree would have FOP read a file from elsewhere
/// and write its contents into the repository, where a commit publishes them;
/// links between lists in the same tree are fine.
fn link_in_tree(path: &Path, root: &Path) -> bool {
    fs::canonicalize(path).is_ok_and(|target| target.starts_with(root) && target.is_file())
}

/// Whether `path` is a list FOP may read and rewrite under `root` (canonical):
/// a regular file, or a symlink that stays in the tree.
fn list_file_in_tree(path: &Path, root: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => link_in_tree(path, root),
        Ok(meta) => meta.is_file(),
        Err(_) => false,
    }
}

/// `path` canonicalised, for comparing link targets against.
fn canonical_root(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Check if a file should use localhost mode
#[inline]
fn is_localhost_file(path: &Path, localhost: bool, localhost_files: &[String]) -> bool {
    localhost || {
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        localhost_files.iter().any(|f| fname == f.as_str())
            || localhost_files.iter().any(|f| path.ends_with(f.as_str()))
    }
}

/// Check if a file should use AdGuard parsing
#[inline]
fn is_adguard_file(path: &Path, parse_adguard: bool, parse_adguard_files: &[String]) -> bool {
    parse_adguard || {
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        parse_adguard_files.iter().any(|f| fname == f.as_str())
            || parse_adguard_files.iter().any(|f| path.ends_with(f.as_str()))
    }
}

/// Get list of changed/untracked files from git
/// Returns None if git not available or not in a repo
#[inline]
fn get_git_changed_files(location: &Path) -> Option<ahash::AHashSet<PathBuf>> {
    use std::process::Command;
    
    // Get changed files - if this fails, git isn't available or not a repo
    let output = Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(location)
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None; // Not a git repo or git not installed
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // If no changes, return empty set (skip all files)
    if stdout.trim().is_empty() {
        return Some(ahash::AHashSet::new());
    }
    
    let files: ahash::AHashSet<PathBuf> = stdout
        .lines()
        .filter_map(|line| {
            // Format: "XY filename" or "XY original -> renamed"
            let path_str = line.get(3..)?.trim();
            // Handle renames: "old -> new"
            let path_str = path_str.split(" -> ").last()?;
            Some(location.join(path_str))
        })
        .collect();
    
    Some(files)
}

/// Which commit a CI run should diff against.
///
/// `origin/master` on a pull request. When HEAD already matches it -- a push
/// to the branch itself -- there is nothing between them, so the last commit
/// is what arrived.
fn ci_diff_base(base_cmd: &[String]) -> Option<String> {
    // The default branch is read from the remote rather than assumed to be
    // `master`: on a `main` repository the assumed ref does not resolve, the
    // diff fails, and an audit built on it reports nothing wrong.
    let resolves = |r: &str| {
        std::process::Command::new(&base_cmd[0])
            .args(&base_cmd[1..])
            .args(["rev-parse", "--verify", "--quiet", r])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let last_commit = || resolves("HEAD~1").then(|| "HEAD~1".to_string());
    // No default branch at all -- a shallow PR checkout fetches only the merge
    // ref -- still leaves the last commit, which is what arrived. A `?` here
    // returned before this fallback could run, and the audit failed the build.
    let Some(default) = fop_git::get_default_branch(base_cmd, "origin") else {
        return last_commit();
    };
    let upstream = format!("origin/{}", default);
    if !resolves(&upstream) {
        return last_commit();
    }
    // HEAD already matching the upstream means this is a push to the branch
    // itself, so what arrived is the last commit.
    let same = std::process::Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["diff", "--quiet", "HEAD", &upstream])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if same {
        return last_commit();
    }
    // From the fork point, not the tip: diffing against the tip made every rule
    // the default branch deleted since the fork show up as a `+` on a branch
    // that is behind, and the audit failed on lines its author never wrote. A
    // shallow clone may not reach the fork point; the tip is the fallback.
    let merge_base = std::process::Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["merge-base", "HEAD", &upstream])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(merge_base.unwrap_or(upstream))
}

/// `git -C <location>`, so a CI audit inspects the repository it was pointed
/// at rather than whatever directory fop was launched from.
fn ci_git_cmd(git_binary: Option<&str>, location: &Path) -> Vec<String> {
    vec![
        git_binary.unwrap_or("git").to_string(),
        "-C".to_string(),
        location.display().to_string(),
    ]
}

/// Run the addition checks, returning false when the caller should stop.
///
/// `interactive` is false in sort-only mode, where there is no commit to
/// confirm and the findings are a report.
#[allow(clippy::too_many_arguments)]
fn run_rule_checks<'c, F>(
    base_cmd: &[String],
    remove_bad_rules: bool,
    dry_run: bool,
    config_for: &F,
    no_color: bool,
    file_extensions: &[String],
    ignore_files: &[String],
    ignore_dirs: &[String],
    ignore_all_but: &[String],
    disable_ignored: bool,
    interactive: bool,
) -> bool
where
    F: Fn(&Path) -> fop_sort::SortConfig<'c> + Sync + ?Sized,
{
    // Relative to the repository root, as the diff reports paths. Without a
    // root the file name alone still finds per-file settings.
    let root = fop_git::repo_root(base_cmd).unwrap_or_default();
    // Filter lists only -- the diff also carries workflows, scripts and
    // source, where a `$` is not an option marker and --remove-bad-rules would
    // delete a working line.
    let gather = || -> Option<Vec<fop_typos::Addition>> {
        Some(
            fop_git::get_added_lines(base_cmd)?
                .into_iter()
                .filter(|a| {
                    diff_path_is_filter_list(
                        &a.file,
                        file_extensions,
                        ignore_files,
                        ignore_dirs,
                        ignore_all_but,
                        disable_ignored,
                    )
                })
                .collect(),
        )
    };

    // A failed diff is not an empty one. Saying nothing here would report a
    // clean bill of health for a check that never ran, so it stops a commit
    // and is merely announced when there is no commit to stop.
    let Some(additions) = gather() else {
        eprintln!("Warning: could not read the diff; the rule checks did not run.");
        return !interactive;
    };
    let tidied = tidy_all(&additions, config_for, &root);
    let problems = check_as_sorted(&additions, &tidied);
    if problems.is_empty() {
        return true;
    }
    fop_rules::report_addition_problems(&problems, no_color);
    println!("\nFound {} questionable rule(s) in added lines.", problems.len());

    // A dry run writes nothing, so the lines stay -- but that is a reason to
    // fall through to the prompt, not to report the rules as dealt with. An
    // early `true` here let `--output --remove-bad-rules` commit them.
    if remove_bad_rules && dry_run {
        println!("Dry run: the flagged lines were left in place.");
    }
    if remove_bad_rules && !dry_run {
        // Defects go; advice stays. `removable` exists to draw exactly that
        // line -- a bare hostname or an unanchored host rule is legal syntax,
        // and in a plain domain-list file it is what belongs there -- and the
        // CI audit already honoured it by failing only on defects. Deleting
        // advice too, with a note afterwards, removed 1062 deliberate entries
        // from one such file in a single run. Nothing is rewritten in place: a
        // silent correction is harder to notice than a deletion.
        let advice = problems.iter().filter(|(_, p)| !p.removable).count();
        let removable: Vec<&fop_typos::Addition> =
            problems.iter().filter(|(_, p)| p.removable).map(|(add, _)| *add).collect();
        let (targets, merged) = partition_merged(&removable, base_cmd);
        if !merged.is_empty() {
            eprintln!(
                "\nNot removed -- each of these may hold a committed rule that sorting \
                 merged into it, and deleting the line would delete that rule too:"
            );
            for add in &merged {
                eprintln!("  {}:{}: {}", add.file, add.line_num, add.content);
            }
            eprintln!("Fix them by hand.");
        }
        match remove_flagged_lines(&targets, base_cmd) {
            Ok(n) => {
                println!("Removed {} line(s).", n);
                if advice > 0 {
                    println!(
                        "Kept {} line(s) flagged as advice rather than a defect -- \
                         review them, but they are legal as written.",
                        advice
                    );
                }
            }
            Err(e) => {
                eprintln!("Could not remove flagged lines: {}", e);
                return false;
            }
        }
        // Re-read the diff: `commit -a` will pick up the working tree as it
        // now stands, so anything still flagged would be committed. A diff
        // that cannot be read is not a clean one.
        let Some(after) = gather() else {
            eprintln!("Warning: could not re-read the diff after removing lines.");
            return !interactive;
        };
        // Only a defect still present is a failure; the advice was kept on
        // purpose above.
        let after_tidied = tidy_all(&after, config_for, &root);
        let left = check_as_sorted(&after, &after_tidied)
            .iter()
            .filter(|(_, p)| p.removable)
            .count();
        if left > 0 {
            if interactive {
                eprintln!("{} rule(s) could not be removed; stopping rather than committing them.", left);
            } else {
                eprintln!("{} rule(s) could not be removed.", left);
            }
            return false;
        }
        return true;
    }

    if !interactive {
        return true;
    }
    print!("Continue with commit? (y/N): ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    if input.trim().to_lowercase() != "y" {
        println!("Commit aborted. Fix the rules and try again.");
        return false;
    }
    true
}

/// Each added line in the form the sort will write it (see `fop_sort::tidy_rule`),
/// under the config its own file is sorted with.
///
/// On the rayon pool: this is the sort's per-line work, and serially it cost
/// the checks half again their time when a large block of rules was added.
fn tidy_all<'c, F>(additions: &[fop_typos::Addition], config_for: &F, root: &Path) -> Vec<String>
where
    F: Fn(&Path) -> fop_sort::SortConfig<'c> + Sync + ?Sized,
{
    additions
        .par_iter()
        .map(|add| {
            let config = config_for(&root.join(&add.file));
            fop_sort::tidy_rule(&add.content, &config).into_owned()
        })
        .collect()
}

/// The rule checks, judged on each addition's as-sorted form but reported
/// against the line the author wrote -- which is also the one removal deletes.
fn check_as_sorted<'a>(
    additions: &'a [fop_typos::Addition],
    tidied: &'a [String],
) -> Vec<(&'a fop_typos::Addition, fop_rules::RuleProblem<'a>)> {
    additions
        .iter()
        .zip(tidied)
        .filter_map(|(add, as_sorted)| fop_rules::check_rule(as_sorted).map(|p| (add, p)))
        .collect()
}

/// Split flagged lines into those safe to delete and those that may carry a
/// committed rule.
///
/// The checks run before sorting, so a flagged line is normally just what the
/// author wrote. But a file already sorted since HEAD -- by an earlier
/// `--no-commit` run, say -- may hold a merge: a committed `a.com##.ad` and an
/// added `b..com##.ad` become `a.com,b..com##.ad`, and deleting that line
/// deletes the committed rule with it. So a line is held back when a committed
/// line sharing its merge key has gone missing from the file: that rule was
/// merged into something, and this line may be it. A file that cannot be read
/// cannot be vouched for, so its lines are held back too.
fn partition_merged<'a>(
    targets: &[&'a fop_typos::Addition],
    base_cmd: &[String],
) -> (Vec<&'a fop_typos::Addition>, Vec<&'a fop_typos::Addition>) {
    let root = fop_git::repo_root(base_cmd);
    let mut absorbed: HashMap<&str, Option<HashSet<String>>> = HashMap::new();
    for add in targets {
        absorbed.entry(add.file.as_str()).or_insert_with(|| {
            let current = fs::read_to_string(root.as_ref()?.join(&add.file)).ok()?;
            let present: HashSet<&str> = current.lines().map(str::trim).collect();
            let committed = match fop_git::file_at_head(base_cmd, &add.file) {
                fop_git::AtHead::Content(content) => content,
                // Confirmed new: it has no committed rules to lose.
                fop_git::AtHead::Absent => String::new(),
                // Cannot be vouched for, so nothing in it is deleted.
                fop_git::AtHead::Unknown => return None,
            };
            Some(
                committed
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('!') && !present.contains(l))
                    .map(|l| fop_rules::merge_key(l).into_owned())
                    .collect(),
            )
        });
    }
    targets.iter().partition(|add| {
        absorbed
            .get(add.file.as_str())
            .and_then(|keys| keys.as_ref())
            .is_some_and(|keys| !keys.contains(fop_rules::merge_key(&add.content).as_ref()))
    })
}

/// Delete the flagged lines from their files.
///
/// Grouped per file and applied highest line number first, so removing one
/// line cannot shift the position of the next. The content is compared before
/// deleting: the diff was read moments ago, but if the file moved underneath
/// us it is better to skip the line than to delete the wrong one.
fn remove_flagged_lines(
    targets: &[&fop_typos::Addition],
    base_cmd: &[String],
) -> io::Result<usize> {
    let root = fop_git::repo_root(base_cmd).ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "could not resolve the repository root")
    })?;
    let tree = canonical_root(&root);
    let mut by_file: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for add in targets {
        by_file
            .entry(add.file.as_str())
            .or_default()
            .push((add.line_num, add.content.as_str()));
    }

    let mut removed = 0;
    for (file, mut targets) in by_file {
        targets.sort_unstable_by_key(|&(line_num, _)| std::cmp::Reverse(line_num));
        let path = root.join(file);
        if !list_file_in_tree(&path, &tree) {
            eprintln!("Skipped {}: not a file inside the repository", file);
            continue;
        }
        // One unreadable file must not abandon the rest, nor discard the count
        // of what was already rewritten.
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipped {}: {}", file, e);
                continue;
            }
        };
        let mut lines: Vec<&str> = content.lines().collect();
        let mut cut = 0;
        for (line_num, expected) in targets {
            match line_num.checked_sub(1).and_then(|i| lines.get(i)) {
                Some(actual) if actual.trim() == expected.trim() => {
                    lines.remove(line_num - 1);
                    cut += 1;
                }
                _ => eprintln!(
                    "Skipped {}:{} — the line no longer matches what was flagged.",
                    file, line_num
                ),
            }
        }
        // Nothing matched: leave the file alone rather than rewrite it
        // byte-identically and touch its mtime.
        if cut == 0 {
            continue;
        }
        // `lines()` drops the `\r` of a CRLF file; rejoining with `\n` would
        // rewrite every line in it as a side effect of removing one.
        let newline = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let mut out = lines.join(newline);
        if content.ends_with('\n') {
            out.push_str(newline);
        }
        match fs::write(&path, out) {
            Ok(()) => removed += cut,
            Err(e) => eprintln!("Could not write {}: {}", file, e),
        }
    }
    Ok(removed)
}

#[allow(clippy::too_many_arguments)]
fn process_location(
    location: &Path,
    no_commit: bool,
    no_msg_check: bool,
    disable_ignored: bool,
    no_color: bool,
    no_large_warning: bool,
    ignore_files: &[String],
    ignore_dirs: &[String],
    ignore_all_but: &[String],
    file_extensions: &[String],
    sort_config: &SortConfig,
    create_pr: &Option<String>,
    git_pr_branch: &Option<String>,
    pr_show_changes: bool,
    banned_domains: &Option<ahash::AHashSet<String>>,
    auto_banned_remove: bool,
    direct_push_users: &[String],
    banned_list_file: Option<&str>,
    fix_typos: bool,
    fix_typos_on_add: bool,
    check_rules_on_add: bool,
    remove_bad_rules: bool,
    auto_fix: bool,
    only_sort_changed: bool,
    rebase_on_fail: bool,
    commit_mask: Option<u8>,
    commit_mask_users: &[String],
    commit_mask_bare: bool,
    commit_mask_exempt_hosts: &[String],
    commit_url_template: Option<&str>,
    ci: bool,
    quiet: bool,
    limited_quiet: bool,
    output_diff_individual: bool,
    diff_output: &std::sync::Mutex<Vec<String>>,
    git_message: &Option<String>,
    history: &[String],
    git_binary: Option<&str>,
    add_checksum: &[String],
    validate_checksum_and_fix: &[String],
    add_timestamp: &[String],
    localhost: bool,
    localhost_files: &[String],
    parse_adguard_files: &[String],
    file_overrides: &ahash::AHashMap<String, FileOverrides>,
) -> io::Result<()> {
    if !location.is_dir() {
        eprintln!("{} does not exist or is not a folder.", location.display());
        return Ok(());
    }
    // Detect repository type. Needed without a commit too when the rule checks
    // are on: they read the diff to find what was added.
    let mut repository: Option<&RepoDefinition> = None;
    if !no_commit || check_rules_on_add {
        for repo_type in REPO_TYPES {
            if location.join(repo_type.directory).is_dir() {
                repository = Some(repo_type);
                break;
            }
        }
    }

    // Check initial repository state
    let (base_cmd, original_difference) = if let Some(repo) = repository {
        let base_cmd = build_base_command(repo, location, git_binary);
        match check_repo_changes(&base_cmd, repo) {
            Some(diff) => (Some(base_cmd), diff),
            None => {
                // In sort-only mode the repository is consulted purely for the
                // rule checks, which report their own skip -- saying it twice
                // for one condition helps nobody.
                if !no_commit {
                    eprintln!(
                        "The repository command was unable to run; FOP will not attempt to use repository tools."
                    );
                }
                (None, false)
            }
        }
    } else {
        (None, false)
    };

    if !quiet {
        if no_color {
            println!("\nPrimary location: {}", location.display());
        } else {
            println!("\n{} {}", "Primary location:".bold(), location.display());
        }
    }

    // Collect directories and files
    let entries: Vec<_> = WalkDir::new(location)
        .min_depth(0)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.')
                && (disable_ignored || !IGNORE_DIRS.contains(&name.as_ref()))
                && !should_ignore_dir(e.path(), ignore_dirs)
        })
        .filter_map(|e| e.ok())
        .collect();

    // Print directories first (sequential for ordered output)
    for entry in &entries {
        let path = entry.path();
        if entry_is_dir(entry) && !quiet && !limited_quiet {
            if no_color {
                println!("Current directory: {}", path.display());
            } else {
                println!("{} {}", "Current directory:".bold(), path.display());
            }
        }
    }

    // Collect text files to process
    let root = canonical_root(location);
    let txt_files: Vec<_> = entries
        .iter()
        .filter(|entry| {
            let path = entry.path();
            if entry_is_dir(entry) {
                return false;
            }
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let wanted = file_extensions.iter().any(|ext| ext == extension)
                && (disable_ignored || !IGNORE_FILES.contains(&filename))
                && !should_ignore_file(filename, ignore_files)
                && (ignore_all_but.is_empty()
                    || ignore_all_but.iter().any(|f| filename.contains(f)));
            // Checked last, so only a file that would have been sorted warns
            if wanted && entry.file_type().is_symlink() && !link_in_tree(path, &root) {
                write_warning(&format!(
                    "Skipped {}: a symlink to something outside {}",
                    path.display(),
                    location.display()
                ));
                return false;
            }
            wanted
        })
        .collect();

    // The config a file is actually sorted with: the global settings, the
    // per-file AdGuard and hosts lists, and any [filename] section in
    // .fopconfig. One definition for the sort and the pre-sort rule checks,
    // which judge each line as the sort will write it and so must agree with
    // it file by file -- judged under the global config, a hosts entry in a
    // `localhost_files` file had its space stripped and read as a bare domain.
    let file_config = |path: &Path| -> SortConfig {
        let mut config = SortConfig {
            convert_ubo: sort_config.convert_ubo,
            no_sort: sort_config.no_sort,
            alt_sort: sort_config.alt_sort,
            parse_adguard: is_adguard_file(path, sort_config.parse_adguard, parse_adguard_files),
            localhost: is_localhost_file(path, sort_config.localhost, localhost_files),
            comment_chars: sort_config.comment_chars,
            backup: sort_config.backup,
            keep_empty_lines: sort_config.keep_empty_lines,
            ignore_dot_domains: sort_config.ignore_dot_domains,
            fix_typos,
            ignore_line_minimum: sort_config.ignore_line_minimum,
            abp_convert: sort_config.abp_convert,
            adguard_convert: sort_config.adguard_convert,
            convert_trusted: sort_config.convert_trusted,
            quiet,
            no_color,
            dry_run: sort_config.dry_run,
            output_changed: sort_config.output_changed,
            add_timestamp: sort_config.add_timestamp,
            benchmark: sort_config.benchmark,
        };
        // Apply per-file overrides from [filename] sections in .fopconfig
        if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(overrides) = file_overrides.get(fname) {
                overrides.apply_to(&mut config);
            }
        }
        config
    };

    // Check newly added rules before sorting. The sort merges rules -- an added
    // `b..com##.ad` joins a committed `a.com##.ad` as `a.com,b..com##.ad` -- and
    // checks run after it judged that merged line, so --remove-bad-rules
    // deleted the committed rule along with the bad one. Before the sort, the
    // diff holds only what the author wrote; each line is still judged in the
    // form the sort will write it (see `fop_sort::tidy_rule`). This is also
    // before the timestamp and checksum passes, which hash the file body.
    let mut rules_ok = true;
    if check_rules_on_add {
        match base_cmd.as_ref().filter(|c| fop_git::git_binary_available(&c[0])) {
            Some(base_cmd) => {
                rules_ok = run_rule_checks(
                    base_cmd,
                    remove_bad_rules,
                    sort_config.dry_run,
                    &file_config,
                    no_color,
                    file_extensions,
                    ignore_files,
                    ignore_dirs,
                    ignore_all_but,
                    disable_ignored,
                    !no_commit,
                );
            }
            // Two different failures, and blaming the wrong one sends people
            // hunting: no `.git` here means fop was pointed at a subdirectory,
            // which is a different problem from git being unrunnable.
            None if repository.is_none() => eprintln!(
                "Warning: no repository in {} -- fop looks for .git in the directory it is \
                 given, so run it from the repository root. Skipping the rule checks.",
                location.display()
            ),
            None => eprintln!(
                "Warning: git could not be run; skipping the rule checks."
            ),
        }
    }

    // Get list of changed files from git (if flag enabled)
    let changed_files: Option<HashSet<PathBuf>> = if only_sort_changed {
        get_git_changed_files(location).map(|v| v.into_iter().collect())
    } else {
        None
    };
    
    if !quiet {
        if let Some(ref files) = changed_files {
            println!("Git detected: processing {} changed file(s)", files.len());
        } else if only_sort_changed {
            eprintln!("Warning: --only-sort-changed set but git not available, processing all files");
        }
    }

    // Process files in parallel
    let diffs: Vec<String> = txt_files
        .par_iter()
        .filter_map(|entry| {
        // Skip files git says are unchanged
        if let Some(ref changed) = changed_files {
            if !changed.contains(entry.path()) {
                return None;
            }
        }

        let path = entry.path();
        let config = file_config(path);

        match fop_sort(path, &config) {
            Ok(Some(diff)) => {
                if output_diff_individual {
                    // Individual mode: write .diff file alongside source
                    let diff_path = entry.path().with_extension("diff");
                    if let Err(e) = fop_sort::write_file_no_follow(&diff_path, diff.as_bytes()) {
                        eprintln!("Error writing diff file {}: {}", diff_path.display(), e);
                    } else if !quiet {
                        println!("Diff written to: {}", diff_path.display());
                    }
                    None
                } else {
                    // Combined mode: return diff for collection outside the parallel loop
                    Some(diff)
                }
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("Error processing {}: {}", entry.path().display(), e);
                None
            }
        }
        })
        .collect();

    // Single lock acquisition (reduces mutex pressure)
    if !output_diff_individual && !diffs.is_empty() {
        diff_output.lock().unwrap().extend(diffs);
    }

    // Warn about CRLF files
    let crlf_count = CRLF_FILES.swap(0, std::sync::atomic::Ordering::Relaxed);
    if crlf_count > 0 && !quiet {
        if no_color {
            println!("Warning: {} file(s) contain Windows line endings (CRLF), converting to Unix (LF).", crlf_count);
            println!("  Tip: Set 'git config core.autocrlf input' or add '* text eol=lf' to .gitattributes");
        } else {
            use owo_colors::OwoColorize;
            println!("{} {} file(s) contain Windows line endings (CRLF), converting to Unix (LF).",
                "Warning:".yellow(), crlf_count);
            println!("  {}", "Tip: Set 'git config core.autocrlf input' or add '* text eol=lf' to .gitattributes".white());
        }
    }

    // Delete backup and temp files (sequential, usually few files). A symlink
    // is removed as a link, never followed, so a planted one goes too.
    for entry in &entries {
        let path = entry.path();
        if entry.file_type().is_file() || entry.file_type().is_symlink() {
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if extension == "orig" || extension == "temp" {
                let _ = fs::remove_file(path);
            }
        }
    }


    // Add timestamps to specified files (after sorting, before checksum).
    // Not in a dry run (--benchmark, --output-diff, --output), which promises
    // to leave the files as they were.
    if !sort_config.dry_run && !add_timestamp.is_empty() {
        for entry in &entries {
            if entry_is_file(entry, &root) {
                let path = entry.path();
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                let file_override_timestamp = file_overrides.get(filename).and_then(|o| o.add_timestamp) == Some(true);
                if file_override_timestamp
                    || add_timestamp.iter().any(|f| filename == f.as_str())
                    || add_timestamp.iter().any(|f| path.ends_with(f.as_str()))
                {
                    let is_localhost = is_localhost_file(path, localhost, localhost_files);
                    let _ = fop_datestamp::add_timestamp(path, is_localhost, quiet, no_color);
                }
            }
        }
    }

    // Add checksums to specified files (after sorting, before commit)
    if !sort_config.dry_run && !add_checksum.is_empty() {
        for entry in &entries {
            if entry_is_file(entry, &root) {
                let path = entry.path();
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                let file_override_checksum = file_overrides.get(filename).and_then(|o| o.add_checksum) == Some(true);
                if file_override_checksum
                    || add_checksum.iter().any(|f| filename == f.as_str())
                    || add_checksum.iter().any(|f| path.ends_with(f.as_str()))
                {
                    let is_localhost = is_localhost_file(path, localhost, localhost_files);
                    match fop_checksum::add_checksum(path, is_localhost, quiet, no_color) {
                        Ok(Some(_checksum)) => {
                            // File was modified, checksum written successfully
                        }
                        Ok(None) => {
                            // File unchanged, checksum already correct
                        }
                        Err(e) => {
                            eprintln!("Error adding checksum to {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
    }


    // Validate and fix checksums (after sorting, before commit)
    if !sort_config.dry_run && !validate_checksum_and_fix.is_empty() {
        for entry in &entries {
            if entry_is_file(entry, &root) {
                let path = entry.path();
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if validate_checksum_and_fix.iter().any(|f| filename == f.as_str())
                    || validate_checksum_and_fix.iter().any(|f| path.ends_with(f.as_str()))
                {
                    match fop_checksum::verify_checksum(path) {
                        Ok(fop_checksum::ChecksumResult::Valid) => {
                            if !quiet {
                                println!("Checksum OK: {}", path.display());
                            }
                        }
                        Ok(fop_checksum::ChecksumResult::Invalid { expected, found }) => {
                            if !quiet {
                                eprintln!("Checksum INVALID: {} (expected {}, found {}) - fixing...",
                                    path.display(), expected, found);
                            }
                            let is_localhost = is_localhost_file(path, localhost, localhost_files);
                            if let Err(e) = fop_checksum::add_checksum(path, is_localhost, quiet, no_color) {
                                eprintln!("Error fixing checksum for {}: {}", path.display(), e);
                            }
                        }
                        Ok(fop_checksum::ChecksumResult::Missing) => {
                            if !quiet {
                                eprintln!("Checksum MISSING: {} - adding...", path.display());
                            }
                            let is_localhost = is_localhost_file(path, localhost, localhost_files);
                            if let Err(e) = fop_checksum::add_checksum(path, is_localhost, quiet, no_color) {
                                eprintln!("Error adding checksum for {}: {}", path.display(), e);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
    }

    // Offer to commit changes (skip if no_commit mode)
    if !no_commit {
        if let (Some(repo), Some(base_cmd)) = (repository, base_cmd) {
            if !git_available() {
                eprintln!("Error: git not found in PATH");
                return Ok(());
            }

            // Before anything else that can ask a question: a run already
            // stopped by the rule checks must not go on to prompt about typos
            // and then return silently having committed nothing.
            if !rules_ok {
                return Ok(());
            }

            // Check for typos in added lines
            // Get added lines once for both typo and banned domain checks
            // Fetched here, after the rule checks have already removed what
            // they were going to: a snapshot taken before that would name
            // lines no longer on disk, and the banned-domain scan below would
            // abort the commit over one. `check_rules_on_add` is not in this
            // condition -- run_rule_checks reads its own diff.
            let additions = if fix_typos_on_add || banned_domains.as_ref().is_some_and(|b| !b.is_empty()) {
                get_added_lines(&base_cmd)
            } else {
                None
            };

            if fix_typos_on_add {
                if let Some(ref additions) = additions {
                    let typos = fop_typos::check_additions(additions);
                    if !typos.is_empty() {
                        fop_typos::report_addition_typos(&typos, no_color);
                        println!("\nFound {} typo(s) in added lines.", typos.len());
                        if !auto_fix {
                            print!("Continue with commit? (y/N): ");
                            io::stdout().flush().ok();
                            let mut input = String::new();
                            io::stdin().read_line(&mut input).ok();
                            if input.trim().to_lowercase() != "y" {
                                println!("Commit aborted. Fix typos and try again.");
                                return Ok(());
                            }
                        } else {
                            println!("Auto-fix enabled, continuing...");
                        }
                    }
                }
            }

           // Check for banned domains in added lines
           if let Some(ref banned) = banned_domains {
                if !banned.is_empty() {
                    if let Some(ref additions) = additions {
                        for add in additions {
                            // Skip the banned list file itself
                            if let Some(banned_file) = banned_list_file {
                                if add.file.ends_with(banned_file) {
                                    continue;
                                }
                            }
                            if let Some(domain) = fop_sort::check_banned_domain(&add.content, banned) {
                                eprintln!("Warning: Banned domain in new addition: {} in rule: {}", domain, add.content);
                                if let Ok(mut changes) = fop_sort::SORT_CHANGES.lock() {
                                    changes.banned_domains_found.push((domain, add.content.clone(), add.file.clone()));
                                }
                            }
                        }
                    }
                }
            }

            if let Some(pr_title) = create_pr {
                // Check if user can bypass PR requirement
                let can_direct_push = if !direct_push_users.is_empty() {
                    if let Some(username) = get_git_username() {
                        direct_push_users.contains(&username)
                    } else {
                        false
                    }
                } else {
                    false
                };

                if can_direct_push {
                    // Direct push for authorized users
                    if !quiet {
                        println!("Direct push authorized for user.");
                    }
                    commit_changes(repo, &base_cmd, original_difference, no_msg_check, no_color, no_large_warning, quiet, limited_quiet, rebase_on_fail, git_message, history, commit_mask, commit_mask_users, commit_mask_bare, commit_mask_exempt_hosts, commit_url_template)?;
                } else {
                // Use provided title or prompt
                let message = if !pr_title.is_empty() {
                    pr_title.clone()
                } else {
                    print!("Enter PR commit message: ");
                    io::stdout().flush().ok();
                    let mut msg = String::new();
                    io::stdin().read_line(&mut msg).ok();
                    msg.trim().to_string()
                };

                // Get remote name (origin if exists, otherwise prompt)
                let remote = match get_remote_name(&base_cmd, no_color) {
                    Some(r) => r,
                    None => {
                        eprintln!("No remote available for PR creation.");
                        return Ok(());
                    }
                };
                
                // Determine base branch - use provided or prompt user
                let base_branch = git_pr_branch.clone();

                // Check for banned domains before creating PR
                if !check_banned_domains(no_color, auto_banned_remove, &base_cmd, ci) {
                    return Ok(());
                }
                
                create_pull_request(repo, &base_cmd, &message, &remote, &base_branch, quiet, pr_show_changes, no_color)?;
                }
            } else {

                // Check for banned domains before commit
                if !check_banned_domains(no_color, auto_banned_remove, &base_cmd, ci) {
                    return Ok(());
                }

                commit_changes(
                    repo,
                    &base_cmd,
                    original_difference,
                    no_msg_check,
                    no_color,
                    no_large_warning,
                    quiet,
                    limited_quiet,
                    rebase_on_fail,
                    git_message,
                    history,
                    commit_mask,
                    commit_mask_users,
                    commit_mask_bare,
                    commit_mask_exempt_hosts,
                    commit_url_template,
                )?;
            }
        }
    }

    Ok(())
}

fn print_greeting(no_commit: bool, no_color: bool, config_path: Option<&str>, banned_info: Option<(usize, &str)>) {

    let mode = if no_commit { " (sort only)" } else { "" };
    let version_line = format!("FOP (Filter Orderer and Preener) version {}{}", VERSION, mode);
    let copyright = "Copyright (C) 2025 FanboyNZ";
    let url = "https://github.com/ryanbr/fop-rs (GPL-3.0)";
    let config_line = config_path.map(|p| format!("Using config file: {}", p));
    let banned_line = banned_info.map(|(count, file)| format!("Loaded {} banned domains from {}", count, file));

    if no_color {
        let separator = "=".repeat(version_line.len());
        println!("{}", separator);
        println!("{}", version_line);
        println!("{} - {}", copyright, url);
        if let Some(ref cfg) = config_line {
            println!("{}", cfg);
        }
        if let Some(ref banned) = banned_line {
            println!("{}", banned);
        }
        println!("{}", separator);
    } else {
        let logo = [
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} ",
            "\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255D} \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}",
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255D}",
            "\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{255D}   \u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{255D} ",
            "\u{2588}\u{2588}\u{2551}       \u{255A}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255D} \u{2588}\u{2588}\u{2551}     ",
            "\u{255A}\u{2550}\u{255D}        \u{255A}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255D}  \u{255A}\u{2550}\u{255D}     ",
        ];
        let info = [
            version_line.as_str(),
            copyright,
            url,
            config_line.as_deref().unwrap_or(""),
            banned_line.as_deref().unwrap_or(""),
            "",
        ];

        println!();
        for (logo_line, info_line) in logo.iter().zip(info.iter()) {
            println!("{}  {}", logo_line.white(), info_line);
        }
    }
}

/// Print `--benchmark` results. `times` holds the timed runs, not the
/// warm-up; throughput is taken from the median, which one slow run cannot
/// drag the way it drags a mean.
fn print_benchmark(times: &[std::time::Duration], files: usize, lines: usize, bytes: u64) {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let median = if n % 2 == 1 { sorted[n / 2] } else { (sorted[n / 2 - 1] + sorted[n / 2]) / 2 };
    let mb = bytes as f64 / 1_048_576.0;

    println!();
    println!("FOP Benchmark Results");
    println!("=====================");
    println!("Runs:          {} (after 1 warm-up)", n);
    println!("Threads:       {}", rayon::current_num_threads());
    println!("Files:         {}", files);
    println!("Lines:         {}", lines);
    println!("Size:          {:.5} MB", mb);
    println!();
    for (i, t) in times.iter().enumerate() {
        println!("  Run {}: {:.5}s", i + 1, t.as_secs_f64());
    }
    println!();
    println!("Median:        {:.5}s", median.as_secs_f64());
    println!("Min:           {:.5}s", sorted[0].as_secs_f64());
    println!("Max:           {:.5}s", sorted[n - 1].as_secs_f64());
    println!();
    let secs = median.as_secs_f64();
    if secs > 0.0 {
        println!("Throughput:    {:.5} lines/sec", lines as f64 / secs);
        println!("               {:.5} MB/sec", mb / secs);
        if files > 0 {
            println!("               {:.5}ms/file", secs * 1000.0 / files as f64);
        }
    }
}

fn main() {
    let (mut args, config_path) = Args::parse();

    // Warn early if commit-url-template is missing the {sha} placeholder —
    // the URL would otherwise be built without the commit hash.
    if let Some(ref t) = args.commit_url_template {
        if !t.contains("{sha}") {
            eprintln!(
                "Warning: commit-url-template '{}' does not contain {{sha}} — commit URLs will not include the hash.",
                t
            );
        }
    }

    // Handle help and version
    if args.help {
        Args::print_help();
        return;
    }

    if args.version {
        Args::print_version();
        return;
    }

    if args.show_config {
        args.print_config(config_path.as_deref());
        return;
    }

    // Size the rayon pool before anything touches it.
    //
    // Work is parallelised one task per file, but the default pool is one
    // worker per core regardless of the workload, and each worker gets its own
    // mimalloc heap (~1.75 MB) whether or not it does any work. On a 32-core
    // machine that was 74 MB of resident memory to sort a two-line file, and
    // it scales with the machine rather than the input -- a 128-core CI runner
    // pays far more for the same work.
    //
    // Measured on 32 cores. Peak RSS, default vs capped: 2-line file 75 -> 27 MB,
    // 65 files 223 -> 86 MB, 270 files (670k lines) 274 -> 115 MB. Wall time is
    // equal or better on those, since throughput saturates well before 32.
    //
    // The trade-off, stated plainly: a workload of a few large files wants one
    // worker per file, and 10 x 1 MB files run ~0.02s slower at 8 workers than
    // at 16. 8 is chosen because the target workload is a filter-list repo --
    // many small-to-medium files, where 8 measured fastest of all counts tried.
    // RAYON_NUM_THREADS overrides the cap, but parse it here rather than
    // deferring to rayon. Rayon ignores the variable unless it parses as
    // usize >= 1 and falls back to the full core count otherwise, so merely
    // testing that it is *set* would hand an empty or malformed value the
    // uncapped behaviour this cap exists to avoid -- and `RAYON_NUM_THREADS:
    // ${{ inputs.threads }}` with the input unset is a common CI shape.
    //
    // `--threads` (or `threads` in .fopconfig) wins over RAYON_NUM_THREADS:
    // both name the same pool, and fop's own setting is the more specific of
    // the two. Either overrides the cap -- it is a default for the workload fop
    // is usually pointed at, not a limit on what may be asked for.
    let (workers, _) = resolve_workers(args.threads);
    // A failure here leaves the global pool uninitialised, and the first
    // par_iter then lazily builds rayon's own uncapped pool -- the memory
    // profile silently reverts. Say so rather than leave a mystery.
    if let Err(e) = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
    {
        if !args.quiet {
            eprintln!("Warning: could not size the worker pool ({e}); using rayon's default.");
        }
    }

    // Benchmark mode: force dry-run, no-commit, quiet. The checks on git
    // additions are not sorting, so they stay out of the timing.
    if args.benchmark {
        args.no_commit = true;
        args.quiet = true;
        args.check_rules_on_add = false;
        args.remove_bad_rules = false;
        args.fix_typos_on_add = false;
    }

    // Load banned list early so we can show count in greeting
    let banned_domains_early = args.check_banned_list.as_ref().and_then(|list_path| {
        match fop_sort::load_banned_list(list_path) {
            Ok(set) => Some(set),
            Err(e) => {
                eprintln!("Warning: Could not load banned list {}: {}", list_path.display(), e);
                None
            }
        }
    });
    let banned_info = banned_domains_early.as_ref()
        .map(|set| (set.len(), args.check_banned_list.as_ref().unwrap().to_string_lossy().to_string()));

    if !args.quiet {
        print_greeting(args.no_commit, args.no_color, config_path.as_deref(),
            banned_info.as_ref().map(|(count, path)| (*count, path.as_str())));
    }

    // Set warning output path
    if let Some(ref path) = args.warning_output {
        // Clear existing file -- never through a symlink, which would aim the
        // warnings at a file elsewhere. Refused, they go to stderr instead.
        match fop_sort::write_file_no_follow(path, b"") {
            Ok(()) => {
                *WARNING_OUTPUT.lock().unwrap() = Some(path.clone());
                WARNING_TO_FILE.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => eprintln!("Warning: not writing warnings to {}: {}; using stderr", path.display(), e),
        }
    }
    
    // Load banned domain list if specified
    let banned_domains = if let Some(ref path) = args.check_banned_list {
        // Auto-ignore the banned list file
        if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
            if !args.ignore_files.iter().any(|f| f == filename) {
                args.ignore_files.push(filename.to_string());
            }
        }
        banned_domains_early
    } else {
        None
    };

    // Build sort config
    let sort_config = SortConfig {
        convert_ubo: !args.no_ubo_convert,
        no_sort: args.no_sort,
        alt_sort: args.alt_sort,
        parse_adguard: args.parse_adguard,
        localhost: args.localhost,
        comment_chars: &args.comment_chars,
        backup: args.backup,
        keep_empty_lines: args.keep_empty_lines,
        ignore_dot_domains: args.ignore_dot_domains,
        abp_convert: args.abp_convert,
        adguard_convert: args.adguard_convert,
        convert_trusted: args.convert_trusted,
        fix_typos: args.fix_typos,
        ignore_line_minimum: args.ignore_line_minimum,
        quiet: args.quiet,
        no_color: args.no_color,
        dry_run: args.output_diff.is_some() || args.output_diff_individual || args.output_changed || args.benchmark,
        output_changed: args.output_changed,
        add_timestamp: !args.add_timestamp.is_empty(),
        benchmark: args.benchmark,
    };

    let diff_output: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    // Build list of locations to process
    let locations: Vec<PathBuf> = if args.directories.is_empty() {
        env::current_dir().map(|cwd| vec![cwd]).unwrap_or_default()
    } else {
        let mut unique: Vec<PathBuf> = args
            .directories
            .iter()
            .filter_map(|p| fs::canonicalize(p).ok())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        unique.sort();
        unique
    };

    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // CI mode: Check diff for banned domains
    // CI rule audit: the same checks --check-rules-on-add runs locally, but
    // against the committed diff and ending in an exit code rather than a
    // prompt. Defects fail the build; advice is printed and does not.
    if args.ci && args.check_rules_on_add {
        let Some(location) = locations.first() else {
            eprintln!("CI audit: no directory to check.");
            std::process::exit(1);
        };
        let base_cmd = ci_git_cmd(args.git_binary.as_deref(), location);
        let Some(base) = ci_diff_base(&base_cmd) else {
            eprintln!(
                "CI audit: could not resolve a base commit to diff against. The checkout \
                 has no history to compare with -- a shallow clone holds only HEAD. With \
                 actions/checkout, set `fetch-depth: 2` (or 0 for the full history)."
            );
            std::process::exit(1);
        };
        let Some(additions) = fop_git::get_added_lines_against(&base_cmd, Some(&base)) else {
            eprintln!("CI audit: could not read the diff against {}.", base);
            std::process::exit(1);
        };
        // Only filter lists: the diff also carries workflows, scripts and
        // source, where a `$` is not an option marker.
        let additions: Vec<_> = additions
            .into_iter()
            .filter(|a| {
                diff_path_is_filter_list(
                    &a.file,
                    &args.file_extensions,
                    &args.ignore_files,
                    &args.ignore_dirs,
                    &args.ignore_all_but,
                    args.disable_ignored,
                )
            })
            .collect();

        let problems = fop_rules::check_additions(&additions);
        let (defects, advice): (Vec<_>, Vec<_>) =
            problems.iter().partition(|(_, p)| p.removable);

        for (add, p) in &advice {
            println!("Notice: {}:{}: {} ({})", add.file, add.line_num, add.content, p.reason);
        }
        if !defects.is_empty() {
            eprintln!("\n{} bad rule(s) found:", defects.len());
            for (add, p) in &defects {
                let mut why = p.reason.to_string();
                if !p.detail.is_empty() {
                    why = format!("{}: {}", why, p.detail);
                }
                if let Some(s) = p.suggestion {
                    why.push_str(&format!(" -- did you mean {}?", s));
                }
                eprintln!("  {}:{}: {} ({})", add.file, add.line_num, add.content, why);
            }
            std::process::exit(1);
        }
    }

    if let Some(banned) = args.ci.then_some(()).and(banned_domains.as_ref()) {
        let mut found: Vec<(String, String)> = Vec::new();

        let base_cmd = match locations.first() {
            Some(location) => ci_git_cmd(args.git_binary.as_deref(), location),
            None => {
                eprintln!("CI audit: no directory to check.");
                std::process::exit(1);
            }
        };
        let base = match ci_diff_base(&base_cmd) {
            Some(base) => base,
            None => {
                eprintln!(
                "CI audit: could not resolve a base commit to diff against. The checkout \
                 has no history to compare with -- a shallow clone holds only HEAD. With \
                 actions/checkout, set `fetch-depth: 2` (or 0 for the full history)."
            );
                std::process::exit(1);
            }
        };

        // Shares the diff reader rather than keeping a second one. The copy
        // that used to live here had the bugs the shared parser has since had
        // fixed: no `--no-color`, so a colourised diff made the audit pass
        // having read nothing; `+++` treated as a header, so a rule beginning
        // with `+` escaped the check; and no handling for a quoted path.
        let Some(additions) = fop_git::get_added_lines_against(&base_cmd, Some(&base)) else {
            eprintln!("CI audit: could not read the diff against {}.", base);
            std::process::exit(1);
        };
        for add in &additions {
            if should_ignore_file(&add.file, &args.ignore_files) {
                continue;
            }
            if let Some(domain) = fop_sort::check_banned_domain(&add.content, banned) {
                found.push((domain, add.content.clone()));
            }
        }

        if !found.is_empty() {
            eprintln!("\n{} banned domain(s) found:", found.len());
            for (domain, rule) in &found {
                eprintln!("  {} -> {}", domain, rule);
            }
            std::process::exit(1);
        }
    }

    // Validate checksums if requested
    if !args.validate_checksum.is_empty() {
        let mut any_failed = false;

        for location in &locations {
            for entry in WalkDir::new(location)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                let path = entry.path();
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if args.validate_checksum.iter().any(|f| filename == f.as_str())
                    || args.validate_checksum.iter().any(|f| path.ends_with(f.as_str()))
                {
                    match fop_checksum::verify_checksum(path) {
                        Ok(fop_checksum::ChecksumResult::Valid) => {
                            if !args.quiet {
                                println!("Checksum OK: {}", path.display());
                            }
                        }
                        Ok(fop_checksum::ChecksumResult::Invalid { expected, found }) => {
                            eprintln!("Checksum FAILED: {} (expected {}, found {})", path.display(), expected, found);
                            any_failed = true;
                        }
                        Ok(fop_checksum::ChecksumResult::Missing) => {
                            if !args.quiet {
                                eprintln!("Warning: No checksum found in {}", path.display());
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading {}: {}", path.display(), e);
                            any_failed = true;
                        }
                    }
                }
            }
        }

        if any_failed {
            std::process::exit(1);
        }
    }

    // Standalone typo scan and fix mode
    if args.fix_typos {
        let total_typos = AtomicUsize::new(0);
        let files_with_typos = AtomicUsize::new(0);

        for location in &locations {
            let typo_root = canonical_root(location);
            let entries: Vec<_> = WalkDir::new(location)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    !name.starts_with('.')
                        && (args.disable_ignored || !IGNORE_DIRS.contains(&name.as_ref()))
                        && !should_ignore_dir(e.path(), &args.ignore_dirs)
                })
                .filter_map(|e| e.ok())
                .filter(|e| {
                    if !list_file_in_tree(e.path(), &typo_root) {
                        return false;
                    }
                    let ext = e
                        .path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("");
                    let filename = e.path().file_name().and_then(|n| n.to_str()).unwrap_or("");
                    args.file_extensions.iter().any(|fe| fe == ext)
                        && !should_ignore_file(filename, &args.ignore_files)
                })
                .collect();

            entries.par_iter().for_each(|entry| {
                let path = entry.path();
                if let Ok(content) = fs::read_to_string(path) {
                    // Skip files without cosmetic rules
                    if !content.contains('#') {
                        return;
                    }

                    let mut file_modified = false;
                    let mut file_typo_count = 0;
                    let mut new_lines: Vec<String> = Vec::new();

                    for (line_num, line) in content.lines().enumerate() {
                        let (fixed, fixes) = fop_typos::fix_all_typos(line);
                        if !fixes.is_empty() {
                            file_typo_count += 1;
                            file_modified = true;
                            if !args.quiet {
                                let _ = writeln!(
                                    std::io::stdout().lock(),
                                    "{}:{}: {} ? {} ({})",
                                    path.display(),
                                    line_num + 1,
                                    line,
                                    fixed,
                                    fixes.join(", ")
                                );
                            }
                            new_lines.push(fixed);
                        } else {
                            new_lines.push(line.to_string());
                        }
                    }

                    if file_modified {
                        if args.output_diff.is_none() {
                            if let Err(e) = fs::write(path, new_lines.join("\n") + "\n") {
                                eprintln!("Error writing {}: {}", path.display(), e);
                            }
                        }
                        total_typos.fetch_add(file_typo_count, Ordering::Relaxed);
                        files_with_typos.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }

        if !args.quiet {
            let total = total_typos.load(Ordering::Relaxed);
            let files = files_with_typos.load(Ordering::Relaxed);
            if total > 0 {
                println!("\nFixed {} typo(s) in {} file(s)", total, files);
            } else {
                println!("\nNo typos found");
            }
        }
    }

    // Process single file if --check-file specified
    if let Some(ref file_path) = args.check_file {
        if !file_path.is_file() {
            eprintln!("{} does not exist or is not a file.", file_path.display());
            return;
        }

        if !args.quiet {
            println!("Processing file: {}", file_path.display());
        }

        let mut check_file_config = SortConfig {
            localhost: is_localhost_file(file_path, sort_config.localhost, &args.localhost_files),
            parse_adguard: is_adguard_file(file_path, sort_config.parse_adguard, &args.parse_adguard_files),
            ..sort_config
        };
        if let Some(fname) = file_path.file_name().and_then(|n| n.to_str()) {
            if let Some(overrides) = args.file_overrides.get(fname) {
                overrides.apply_to(&mut check_file_config);
            }
        }

        // Benchmark: count lines/bytes for the single file
        let (bench_lines, bench_bytes) = if args.benchmark {
            if let Ok(content) = fs::read_to_string(file_path) {
                (content.lines().count(), content.len() as u64)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        // One untimed warm-up run first: regex compilation and thread start-up
        // are paid once, not by every sort
        let bench_iterations = if args.benchmark { args.benchmark_runs + 1 } else { 1 };
        let mut bench_times: Vec<std::time::Duration> = Vec::with_capacity(bench_iterations);

        for iteration in 0..bench_iterations {
            let iter_start = std::time::Instant::now();

            match fop_sort::fop_sort(file_path, &check_file_config) {
                Ok(Some(diff)) => {
                    if args.output_diff_individual {
                        let diff_path = file_path.with_extension("diff");
                        if let Err(e) = fop_sort::write_file_no_follow(&diff_path, diff.as_bytes()) {
                            eprintln!("Error writing diff file: {}", e);
                        } else if !args.quiet {
                            println!("Diff written to: {}", diff_path.display());
                        }
                    } else {
                        diff_output.lock().unwrap().push(diff);
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("Error processing {}: {}", file_path.display(), e),
            }

            let elapsed = iter_start.elapsed();
            if args.benchmark && iteration > 0 {
                bench_times.push(elapsed);
            }
            if args.benchmark {
                WARNINGS_MUTED.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }

        // Print benchmark results for --check-file
        if args.benchmark {
            print_benchmark(&bench_times, 1, bench_lines, bench_bytes);
        }

        // Add checksum if requested (not in a dry run)
        if !check_file_config.dry_run && !args.add_checksum.is_empty() {
            let filename = file_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
                if args.add_checksum.iter().any(|f| filename == f.as_str())
                    || args.add_checksum.iter().any(|f| file_path.ends_with(f.as_str()))
                {
                let is_localhost = is_localhost_file(file_path, args.localhost, &args.localhost_files);
                let _ = fop_checksum::add_checksum(file_path, is_localhost, args.quiet, args.no_color);
            }
        }

        // Handle git commit (unless no_commit mode)
        if !args.no_commit {
            let parent = file_path.parent().unwrap_or(std::path::Path::new("."));
            if let Some(repo) = REPO_TYPES
                .iter()
                .find(|r| parent.join(r.directory).is_dir())
            {
                let base_cmd = fop_git::build_base_command(repo, parent, args.git_binary.as_deref());

                // Check for banned domains before commit
                if !fop_git::check_banned_domains(args.no_color, args.auto_banned_remove, &base_cmd, args.ci) {
                    return;
                }

                if let Err(e) = fop_git::commit_changes(
                    repo,
                    &base_cmd,
                    false,
                    args.no_msg_check,
                    args.no_color,
                    args.no_large_warning,
                    args.quiet,
                    args.limited_quiet,
                    args.rebase_on_fail,
                    &args.git_message,
                    &args.history,
                    args.commit_mask,
                    &args.commit_mask_users,
                    args.commit_mask_bare,
                    &args.commit_mask_exempt_hosts,
                    args.commit_url_template.as_deref(),
                ) {
                    eprintln!("Git error: {}", e);
                }
            }
        }

        // Write diff if requested
        if let Some(ref diff_path) = &args.output_diff {
            let diffs = diff_output.lock().unwrap();
            if let Err(e) = fop_sort::write_file_no_follow(diff_path, diffs.join("\n").as_bytes()) {
                eprintln!("Error writing diff file: {}", e);
            }
        }
        fop_git::exit_if_unpublished();
        return;
    }

    // Clear any previous tracking data and enable if needed
    if args.pr_show_changes {
        fop_sort::clear_tracked_changes();
        TRACK_CHANGES.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    // Benchmark: count files and lines before processing
    let (bench_files, bench_lines, bench_bytes) = if args.benchmark {
        let mut files = 0usize;
        let mut lines = 0usize;
        let mut bytes = 0u64;
        for location in &locations {
            let root = canonical_root(location);
            for entry in WalkDir::new(location)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    !name.starts_with('.')
                        && (args.disable_ignored || !IGNORE_DIRS.contains(&name.as_ref()))
                        && !should_ignore_dir(e.path(), &args.ignore_dirs)
                })
                .filter_map(|e| e.ok())
            {
                if !entry_is_file(&entry, &root) { continue; }
                let path = entry.path();
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !args.file_extensions.iter().any(|ext| ext == extension) { continue; }
                if !args.disable_ignored && IGNORE_FILES.contains(&filename) { continue; }
                if should_ignore_file(filename, &args.ignore_files) { continue; }
                if !args.ignore_all_but.is_empty()
                    && !args.ignore_all_but.iter().any(|f| filename.contains(f)) { continue; }
                files += 1;
                if let Ok(content) = fs::read_to_string(path) {
                    lines += content.lines().count();
                    bytes += content.len() as u64;
                }
            }
        }
        (files, lines, bytes)
    } else {
        (0, 0, 0)
    };

    // One untimed warm-up run first: regex compilation and thread start-up
    // are paid once, not by every sort
    let bench_iterations = if args.benchmark { args.benchmark_runs + 1 } else { 1 };
    let mut bench_times: Vec<std::time::Duration> = Vec::with_capacity(bench_iterations);

    for iteration in 0..bench_iterations {
        let iter_start = std::time::Instant::now();

        // Process all locations
        for (i, location) in locations.iter().enumerate() {
            if let Err(e) = process_location(
                location,
                args.no_commit,
                args.no_msg_check,
                args.disable_ignored,
                args.no_color,
                args.no_large_warning,
                &args.ignore_files,
                &args.ignore_dirs,
                &args.ignore_all_but,
                &args.file_extensions,
                &sort_config,
                &args.create_pr,
                &args.git_pr_branch,
                args.pr_show_changes,
                &banned_domains,
                args.auto_banned_remove,
                &args.direct_push_users,
                args.check_banned_list.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()),
                args.fix_typos,
                args.fix_typos_on_add,
                args.check_rules_on_add,
                args.remove_bad_rules,
                args.auto_fix,
                args.only_sort_changed,
                args.rebase_on_fail,
                args.commit_mask,
                &args.commit_mask_users,
                args.commit_mask_bare,
                &args.commit_mask_exempt_hosts,
                args.commit_url_template.as_deref(),
                args.ci,
                args.quiet,
                args.limited_quiet,
                args.output_diff_individual,
                &diff_output,
                &args.git_message,
                &args.history,
                args.git_binary.as_deref(),
                &args.add_checksum,
                &args.validate_checksum_and_fix,
                &args.add_timestamp,
                args.localhost,
                &args.localhost_files,
                &args.parse_adguard_files,
                &args.file_overrides,
            ) {
                eprintln!("Error: {}", e);
            }
            // Print blank line between multiple directories (preserve original behavior)
            if !args.benchmark && locations.len() > 1 && i < locations.len() - 1 {
                println!();
            }
        }

        let elapsed = iter_start.elapsed();
        if args.benchmark && iteration > 0 {
            bench_times.push(elapsed);
        }
        if args.benchmark {
            WARNINGS_MUTED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    // Print benchmark results
    if args.benchmark {
        print_benchmark(&bench_times, bench_files, bench_lines, bench_bytes);
    }

    // Clear tracking data when done (free memory)
    if args.pr_show_changes {
        TRACK_CHANGES.store(false, std::sync::atomic::Ordering::Relaxed);
        fop_sort::clear_tracked_changes();
    }

    // Write collected diffs if --output-diff specified
    if let Some(ref diff_path) = &args.output_diff {
        let diffs = diff_output.lock().unwrap();
        if let Err(e) = fop_sort::write_file_no_follow(diff_path, diffs.join("\n").as_bytes()) {
            eprintln!("Error writing diff file: {}", e);
        } else if !args.quiet && !diffs.is_empty() {
            println!("Diff written to: {}", diff_path.display());
        }
    }

    // Flush any buffered warnings to file
    flush_warnings();

    // A commit that could not be published exits 1, now the rest is done
    fop_git::exit_if_unpublished();
}
