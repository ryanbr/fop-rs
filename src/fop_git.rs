//! Git repository operations for FOP

use colored::Colorize;
use regex::Regex;
use std::io::BufRead;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use crate::fop_sort::SORT_CHANGES;
use std::sync::LazyLock;

/// Format changes for PR body
pub fn format_pr_changes() -> String {
    const MAX_ITEMS: usize = 40; // Limit to avoid huge PR bodies
    const MAX_BODY_LEN: usize = 1700;  // Leave room for base URL
    
    if let Ok(changes) = SORT_CHANGES.lock() {
        // Estimate capacity: ~100 chars per item
        let estimated = (changes.typos_fixed.len() 
            + changes.domains_combined.len()
            + changes.has_text_merged.len()
            + changes.duplicates_removed.len()) * 100;
        let mut body = String::with_capacity(estimated.min(8000));

        if !changes.typos_fixed.is_empty() {
            body.push_str("## Typos Fixed\n\n");
            for (before, after, reason) in changes.typos_fixed.iter().take(MAX_ITEMS) {
                body.push_str(&format!("- `{}` -> `{}` ({})\n", before, after, reason));
            }
            if changes.typos_fixed.len() > MAX_ITEMS {
                body.push_str(&format!("- ... and {} more\n", changes.typos_fixed.len() - MAX_ITEMS));
            }
            body.push('\n');
        }
        if !changes.domains_combined.is_empty() {
            body.push_str("## Domains Combined\n\n");
            for (originals, combined) in changes.domains_combined.iter().take(MAX_ITEMS) {
                body.push_str(&format!("- `{}` -> `{}`\n", originals.join("` + `"), combined));
            }
            if changes.domains_combined.len() > MAX_ITEMS {
                body.push_str(&format!("- ... and {} more\n", changes.domains_combined.len() - MAX_ITEMS));
            }
            body.push('\n');
        }
        
        if !changes.has_text_merged.is_empty() {
            body.push_str("## :has-text() Merged\n\n");
            for (originals, merged) in changes.has_text_merged.iter().take(MAX_ITEMS) {
                body.push_str(&format!("- {} rules -> `{}`\n", originals.len(), merged));
            }
            if changes.has_text_merged.len() > MAX_ITEMS {
                body.push_str(&format!("- ... and {} more\n", changes.has_text_merged.len() - MAX_ITEMS));
            }
            body.push('\n');
        }
        
        if !changes.duplicates_removed.is_empty() {
            body.push_str("## Duplicates Removed\n\n");
            let dupes: Vec<_> = changes.duplicates_removed.iter().collect();
            for dup in dupes.iter().take(MAX_ITEMS) {
                body.push_str(&format!("- `{}`\n", dup));
            }
            if changes.duplicates_removed.len() > MAX_ITEMS {
                body.push_str(&format!("- ... and {} more\n", changes.duplicates_removed.len() - MAX_ITEMS));
            }
            body.push('\n');
        }

        // Truncate if too long for URL
        if body.len() > MAX_BODY_LEN {
            body.truncate(MAX_BODY_LEN);
            body.push_str("\n\n... (truncated)\n");
        }
        
        return body;
    }


        
    
    String::new()
}

// =============================================================================
// Repository Definition
// =============================================================================

#[derive(Clone)]
pub struct RepoDefinition {
    pub name: &'static str,
    pub directory: &'static str,
    pub location_option: &'static str,
    pub repo_directory_option: Option<&'static str>,
    pub check_changes: &'static [&'static str],
    pub difference: &'static [&'static str],
    pub commit: &'static [&'static str],
    pub pull: &'static [&'static str],
    pub push: &'static [&'static str],
}

pub const GIT: RepoDefinition = RepoDefinition {
    name: "git",
    directory: ".git",
    location_option: "--work-tree=",
    repo_directory_option: Some("--git-dir="),
    check_changes: &["status", "-s", "--untracked-files=no"],
    difference: &["diff"],
    commit: &["commit", "-a", "-m"],
    pull: &["pull", "--rebase"],
    push: &["push"],
};

pub const REPO_TYPES: &[RepoDefinition] = &[GIT];

// =============================================================================
// Commit Message Validation
// =============================================================================

static COMMIT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(A|M|P):\s((\(.+\))\s)?(.*)$").unwrap());

#[inline]
pub fn valid_url(url_str: &str) -> bool {
    if url_str.starts_with("about:") {
        return true;
    }

    let Some(scheme_end) = url_str.find("://") else {
        return false;
    };

    let scheme = &url_str[..scheme_end];
    if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }

    let rest = &url_str[scheme_end + 3..];
    if rest.is_empty() {
        return false;
    }

    let host_end = rest.find('/').unwrap_or(rest.len());
    !rest[..host_end].is_empty()
}

pub fn check_comment(comment: &str, user_changes: bool) -> bool {
    match COMMIT_PATTERN.captures(comment) {
        None => {
            eprintln!(
                "The comment \"{}\" is not in the recognised format.",
                comment
            );
            false
        }
        Some(caps) => {
            let indicator = &caps[1];
            match indicator {
                "M" => true,
                "A" | "P" => {
                    if !user_changes {
                        eprintln!("You have indicated that you have added or removed a rule, but no changes were initially noted by the repository.");
                        false
                    } else {
                        let address = &caps[4];
                        if !valid_url(address) {
                            eprintln!("Unrecognised address \"{}\".", address);
                            false
                        } else {
                            true
                        }
                    }
                }
                _ => false,
            }
        }
    }
}

// =============================================================================
// Repository Commands
// =============================================================================

pub fn build_base_command(repo: &RepoDefinition, location: &Path) -> Vec<String> {
    let mut cmd = vec![repo.name.to_string()];

    if repo.location_option.ends_with('=') {
        cmd.push(format!("{}{}", repo.location_option, location.display()));
    } else {
        cmd.push(repo.location_option.to_string());
        cmd.push(location.display().to_string());
    }

    if let Some(repo_opt) = repo.repo_directory_option {
        let repo_dir = location.join(repo.directory);
        if repo_opt.ends_with('=') {
            cmd.push(format!("{}{}", repo_opt, repo_dir.display()));
        } else {
            cmd.push(repo_opt.to_string());
            cmd.push(repo_dir.display().to_string());
        }
    }

    cmd
}

/// Check if git command is available
#[inline]
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn check_repo_changes(base_cmd: &[String], repo: &RepoDefinition) -> Option<bool> {
    if base_cmd.is_empty() {
        return None;
    }
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(repo.check_changes)
        .output()
        .ok()?;

    Some(!output.stdout.is_empty())
}

pub fn get_diff(base_cmd: &[String], repo: &RepoDefinition) -> Option<String> {
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(repo.difference)
        .output()
        .ok()?;

    String::from_utf8(output.stdout).ok()
}

// =============================================================================
// Diff Display
// =============================================================================

#[inline]
fn is_large_change(diff: &str) -> bool {
    const LARGE_LINES_THRESHOLD: usize = 25;

    let changed_lines = diff
        .lines()
        .filter(|line| {
            (line.starts_with('+') || line.starts_with('-'))
                && !line.starts_with("+++")
                && !line.starts_with("---")
        })
        .count();

    changed_lines > LARGE_LINES_THRESHOLD
}

/// Prompt user to restore changes
fn prompt_restore(base_cmd: &[String], no_color: bool) -> io::Result<bool> {
    if no_color {
        print!("Would you like to restore the previous state before this change? [y/N]: ");
    } else {
        print!("{}", "Would you like to restore the previous state before this change? [y/N]: ".yellow());
    }
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    if input.trim().eq_ignore_ascii_case("y") {
        let status = Command::new(&base_cmd[0])
            .args(&base_cmd[1..])
            .args(["restore", "."])
            .status()?;
        
        if status.success() {
            println!("Changes restored successfully.");
            return Ok(true);
        } else {
            eprintln!("Failed to restore changes.");
        }
    }
    
    Ok(false)
}

#[inline]
fn print_diff_line(line: &str, no_color: bool) {
    if no_color {
        println!("{}", line);
    } else if line.starts_with('+') && !line.starts_with("+++") {
        println!("{}", line.green());
    } else if line.starts_with('-') && !line.starts_with("---") {
        println!("{}", line.red());
    } else {
        println!("{}", line);
    }
}

fn print_diff(diff: &str, no_color: bool) {
    for line in diff.lines() {
        print_diff_line(line, no_color);
    }
}

// =============================================================================
// Pull Request Operations
// =============================================================================

/// Get list of available remotes
#[inline]
fn get_remotes(base_cmd: &[String]) -> Vec<String> {
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .arg("remote")
        .output()
        .ok();

    match output {
        Some(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => vec![],
    }
}

/// Get remote to use - origin if exists, otherwise prompt or use single remote
pub fn get_remote_name(base_cmd: &[String], no_color: bool) -> Option<String> {
    let remotes = get_remotes(base_cmd);
    
    if remotes.is_empty() {
        eprintln!("No remotes found.");
        return None;
    }
    
    // Use origin if available
    if remotes.iter().any(|r| r == "origin") {
        println!("Using remote: origin");
        return Some("origin".to_string());
    }
    
    // Single remote - use it
    if remotes.len() == 1 {
        println!("Using remote: {}", remotes[0]);
        return Some(remotes[0].clone());
    }
    
    // Multiple remotes, no origin - prompt
    prompt_for_remote(&remotes, no_color)
}

/// Get the remote URL for constructing PR link
#[inline]
fn get_remote_url(base_cmd: &[String], remote: &str) -> Option<String> {
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["remote", "get-url", remote])
        .output()
        .ok()?;

    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Get current branch name
#[inline]
fn get_current_branch(base_cmd: &[String]) -> Option<String> {
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout).ok().and_then(|s| {
        let branch = s.trim();
        // In detached HEAD state, git prints "HEAD" (not a real branch name).
        if branch.is_empty() || branch == "HEAD" {
            None
        } else {
            Some(branch.to_string())
        }
    })
}

/// Prompt user to select a remote
fn prompt_for_remote(remotes: &[String], no_color: bool) -> Option<String> {
    println!("Available remotes: {}", 
        if no_color {
            remotes.join(", ")
        } else {
            remotes.iter().map(|s| s.yellow().to_string()).collect::<Vec<_>>().join(", ")
        }
    );
    
    loop {
        print!("Enter remote name: ");
        io::stdout().flush().ok();
        
        let mut input = String::new();
        if io::stdin().lock().read_line(&mut input).is_err() {
            return None;
        }
        let input = input.trim();
        
        if remotes.iter().any(|r| r == input) {
            return Some(input.to_string());
        }
        
        eprintln!("Remote \"{}\" not found. Please try again.", input);
    }
}

/// Convert git remote URL to web URL and generate PR/MR link
fn generate_pr_url(remote: &str, base_branch: &str, pr_branch: &str, body: Option<&str>) -> Option<String> {
    let remote = remote.trim().trim_end_matches(".git");
    
    // Build base URL from SSH or HTTPS format
    let base_url = if let Some(rest) = remote.strip_prefix("git@") {
        // SSH format: git@host:user/repo
        let colon_pos = rest.find(':')?;
        let (host, path) = rest.split_at(colon_pos);
        format!("https://{}/{}", host, &path[1..])
    } else if remote.starts_with("https://") || remote.starts_with("http://") {
        remote.to_string()
    } else {
        return None;
    };
    
    // Detect platform and generate URL (only for known platforms)
    if base_url.contains("gitlab") {
        let mut url = format!("{}/-/merge_requests/new?merge_request[source_branch]={}&merge_request[target_branch]={}", 
            base_url, pr_branch, base_branch);
        if let Some(b) = body {
            url.push_str(&format!("&merge_request[description]={}", urlencoding::encode(b)));
        }
        Some(url)
    } else if base_url.contains("github") {
        let mut url = format!("{}/compare/{}...{}?expand=1", 
            base_url, base_branch, pr_branch);
        if let Some(b) = body {
            url.push_str(&format!("&body={}", urlencoding::encode(b)));
        }
        Some(url)
    } else {
        None
    }
}

/// Switch to a branch
fn checkout_branch(base_cmd: &[String], branch: &str) -> io::Result<bool> {
    Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["checkout", branch])
        .status()
        .map(|s| s.success())
}

/// Get added lines from git diff
pub fn get_added_lines(base_cmd: &[String]) -> Option<Vec<crate::fop_typos::Addition>> {
    use crate::fop_typos::Addition;

    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["diff", "--no-color", "-U0"])
        .output()
        .ok()?;

    let diff = String::from_utf8(output.stdout).ok()?;
    let mut added = Vec::new();
    let mut current_file = String::new();
    let mut line_num: usize = 0;

    for line in diff.lines() {
        if let Some(file) = line.strip_prefix("+++ b/") {
            current_file = file.to_string();
        } else if line.starts_with("@@ ") {
            // Parse line number from @@ -x,y +n,m @@
            if let Some(plus_pos) = line.find(" +") {
                let rest = &line[plus_pos + 2..];
                 if let Some(end) = rest.find([',', ' ']) {
                    line_num = rest[..end].parse().unwrap_or(0);
                }
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            let content = line[1..].to_string();
            if !content.is_empty() {
                added.push(Addition {
                    file: current_file.clone(),
                    line_num,
                    content,
                });
            }
            line_num += 1;
        } else if line.starts_with(' ') {
            // Context line (not present with -U0, but correct if that ever changes)
            line_num += 1;
        } else if line.starts_with('\\') {
            // "\ No newline at end of file" marker; does not advance line numbers
        }
    }

    Some(added)
}

/// Get the default branch name (main, master, etc.) - internal use
#[inline]
fn get_default_branch(base_cmd: &[String], remote: &str) -> Option<String> {
    // Try to get from remote HEAD
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["symbolic-ref", &format!("refs/remotes/{}/HEAD", remote), "--short"])
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let prefix = format!("{}/", remote);
        return branch.strip_prefix(&prefix).map(|s| s.to_string());
    }

    // Fallback: check if main or master exists
    for branch in &["main", "master"] {
        let status = Command::new(&base_cmd[0])
            .args(&base_cmd[1..])
            .args(["show-ref", "--verify", &format!("refs/remotes/{}/{}", remote, branch)])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?;

        if status.success() {
            return Some(branch.to_string());
        }
    }

    None
}

/// Create a pull request branch and return PR URL
#[allow(clippy::too_many_arguments)]
pub fn create_pull_request(
    repo: &RepoDefinition,
    base_cmd: &[String],
    message: &str,
    remote: &str,
    pr_branch_override: &Option<String>,
    quiet: bool,
    show_changes: bool,
    no_color: bool,
) -> io::Result<Option<String>> {
    // Show diff first
    let diff = match get_diff(base_cmd, repo) {
        Some(d) if !d.is_empty() => d,
        _ => {
            println!("\nNo changes have been recorded by the repository.");
            return Ok(None);
        }
    };

    if !quiet {
        println!("\nThe following changes will be included in the PR:");
        print_diff(&diff, no_color);
    }

    // Get current branch (to return to later)
    let current_branch = get_current_branch(base_cmd).unwrap_or_else(|| "master".to_string());

    // Get base branch for PR (user override > auto-detect > current)
    let base_branch = pr_branch_override
        .clone()
        .or_else(|| get_default_branch(base_cmd, remote))
        .unwrap_or_else(|| current_branch.clone());

    // Create branch name with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let pr_branch = format!("fop-update-{}", timestamp);

    if !quiet {
        println!("\nCreating PR branch '{}'...", pr_branch);
    }

    // Create and checkout new branch
    let mut cmd = Command::new(&base_cmd[0]);
    cmd.args(&base_cmd[1..])
        .args(["checkout", "-b", &pr_branch]);
    if quiet {
        cmd.arg("--quiet");
    }
    let status = cmd.status()?;
    if !status.success() {
        eprintln!("Failed to create branch {}", pr_branch);
        return Ok(None);
    }

    // Commit changes
    let status = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(repo.commit)
        .arg(message)
        .status()?;
    if !status.success() {
        eprintln!("Failed to commit changes");
        let _ = checkout_branch(base_cmd, &current_branch);
        return Ok(None);
    }

    // Push branch
    if !quiet {
        println!("Pushing branch to {}...", remote);
    }
    let mut cmd = Command::new(&base_cmd[0]);
    cmd.args(&base_cmd[1..])
        .args(["push", "-u", remote, &pr_branch]);
    if quiet {
        cmd.arg("--quiet");
    }
    let status = cmd.status()?;
    if !status.success() {
        eprintln!("Failed to push branch {}", pr_branch);
        // Switch back to original branch
        let _ = checkout_branch(base_cmd, &current_branch);
        return Ok(None);
    }

    // Switch back to original branch
    let _ = checkout_branch(base_cmd, &current_branch);

    // Build PR body if show_changes enabled
    let pr_body = if show_changes {
        let body = format_pr_changes();
        if body.is_empty() { None } else { Some(body) }
    } else {
        None
    };

    // Generate PR URL
    let pr_url = get_remote_url(base_cmd, remote)
        .and_then(|remote| generate_pr_url(&remote, &base_branch, &pr_branch, pr_body.as_deref()));

    eprintln!("DEBUG: pr_body = {:?}", pr_body);

    if let Some(ref url) = pr_url {
        println!("\n{}", "Pull request branch pushed successfully!".green());
        println!("\nCreate PR at:\n  {}", url.cyan());
    } else {
        println!(
            "\nBranch '{}' pushed. Create PR/MR manually in your git web interface.",
            pr_branch
        );
    }

    Ok(pr_url)
}

// =============================================================================
// Commit Operations
// =============================================================================

/// Attempt rebase and retry push after initial push failure
#[inline]
fn rebase_and_retry_push(base_cmd: &[String], repo: &RepoDefinition) {
    eprintln!("Push failed. Attempting rebase...");
    let rebase_status = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["pull", "--rebase"])
        .status();
    
    if rebase_status.map(|s| s.success()).unwrap_or(false) {
        let retry = Command::new(&base_cmd[0])
            .args(&base_cmd[1..])
            .args(repo.push)
            .status();
        if retry.map(|s| s.success()).unwrap_or(false) {
            println!("Push succeeded after rebase.");
            return;
        }
    }
    eprintln!("Push still failed. Resolve manually.");
}

#[allow(clippy::too_many_arguments)]
pub fn commit_changes(
    repo: &RepoDefinition,
    base_cmd: &[String],
    original_difference: bool,
    no_msg_check: bool,
    no_color: bool,
    no_large_warning: bool,
    quiet: bool,
    rebase_on_fail: bool,
    git_message: &Option<String>,
) -> io::Result<()> {
    let diff = match get_diff(base_cmd, repo) {
        Some(d) if !d.is_empty() => d,
        _ => {
            println!("\nNo changes have been recorded by the repository.");
            return Ok(());
        }
    };

    if !quiet {
        println!("\nThe following changes have been recorded by the repository:");
        print_diff(&diff, no_color);
    }

    // If git message provided via CLI, use it directly
    if let Some(message) = git_message {
        if message.trim().is_empty() {
            eprintln!("Error: Empty commit message provided");
            return Ok(());
        }
        if !no_msg_check && !check_comment(message, original_difference) {
            eprintln!("Error: Invalid commit message format. Use M:/A:/P: prefix.");
            return Ok(());
        }

        if !quiet {
            println!("Committing with message: {}", message);
        }

        Command::new(&base_cmd[0])
            .args(&base_cmd[1..])
            .args(repo.commit)
            .arg(message)
            .status()?;

        // Pull and push
        let mut push_failed = false;
        for (i, op) in [repo.pull, repo.push].iter().enumerate() {
            let mut cmd = Command::new(&base_cmd[0]);
            cmd.args(&base_cmd[1..]).args(*op);
            if quiet {
                cmd.arg("--quiet");
            }
            let status = cmd.status();
            if i == 1 && !status.map(|s| s.success()).unwrap_or(false) {
                push_failed = true;
            }
        }

        if push_failed {
            if rebase_on_fail {
                rebase_and_retry_push(base_cmd, repo);
            } else {
                eprintln!("Push failed. Run 'git pull --rebase' then 'git push'.");
            }
        } else if !quiet {
            if no_color {
                println!("Completed commit process successfully.");
            } else {
                println!(
                    "{}",
                    "Completed commit process successfully.".green().bold()
                );
            }
        }
        return Ok(());
    }

    // Check for large changes
    if !no_large_warning && !original_difference && is_large_change(&diff) {
        if no_color {
            println!("\nThis is a large change. Are you sure you want to proceed?");
            print!("Please type 'YES' to continue: ");
        } else {
            println!(
                "\n{}",
                "This is a large change. Are you sure you want to proceed?".yellow()
            );
            print!("{}", "Please type 'YES' to continue: ".white().bold());
        }
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim() != "YES" {
            println!("Commit aborted.");
            let _ = prompt_restore(base_cmd, no_color);
            return Ok(());
        }
    }

    // Get commit comment
    loop {
        if no_color {
            println!("Please enter a valid commit comment (or ABORT to restore):");
        } else {
            println!(
                "{}",
                "Please enter a valid commit comment (or ABORT to restore):"
                    .white()
                    .bold()
            );
        }
        io::stdout().flush()?;

        let mut comment = String::new();
        if io::stdin().read_line(&mut comment).is_err() {
            println!("\nCommit aborted.");
            return Ok(());
        }

        let comment = comment.trim();
        if comment.is_empty() {
            println!("\nCommit aborted.");
            return Ok(());
        }

 
        // Check for ABORT command
        if comment.eq_ignore_ascii_case("ABORT") {
            println!("Restoring previous state...");
            let status = Command::new(&base_cmd[0])
                .args(&base_cmd[1..])
                .args(["restore", "."])
                .status()?;
            
            if status.success() {
                println!("Changes restored successfully.");
            } else {
                eprintln!("Failed to restore changes.");
            }
            return Ok(());
        }

        if no_msg_check || check_comment(comment, original_difference) {
            if no_color {
                println!("Comment \"{}\" accepted.", comment);
            } else {
                println!(
                    "{} \"{}\" {}",
                    "Comment".green(),
                    comment.cyan(),
                    "accepted.".green()
                );
            }

            // Execute commit
            let status = Command::new(&base_cmd[0])
                .args(&base_cmd[1..])
                .args(repo.commit)
                .arg(comment)
                .status();

            if let Err(e) = status {
                eprintln!("Unexpected error with commit: {}", e);
                return Err(e);
            }

            // Pull and push
            if !quiet {
                if no_color {
                    println!("\nConnecting to server. Please enter your password if required.");
                } else {
                    println!(
                        "\n{}",
                        "Connecting to server. Please enter your password if required.".magenta()
                    );
                }
            }

            let mut push_failed = false;
            for (i, op) in [repo.pull, repo.push].iter().enumerate() {
                let mut cmd = Command::new(&base_cmd[0]);
                cmd.args(&base_cmd[1..]).args(*op);
                if quiet {
                    cmd.arg("--quiet");
                }
                let status = cmd.status();
                if i == 1 && !status.map(|s| s.success()).unwrap_or(false) {
                    push_failed = true;
                }
                if !quiet {
                    println!();
                }
            }

            if push_failed {
                if rebase_on_fail {
                    rebase_and_retry_push(base_cmd, repo);
                } else {
                    eprintln!("Push failed. Run 'git pull --rebase' then 'git push'.");
                }
            } else if !quiet {
                if no_color {
                    println!("Completed commit process successfully.");
                } else {
                    println!(
                        "{}",
                        "Completed commit process successfully.".green().bold()
                    );
                }
            }
            return Ok(());
        }
        println!();
    }
}
