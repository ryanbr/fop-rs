//! Git repository operations for FOP

use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use colored::Colorize;
use regex::Regex;
use once_cell::sync::Lazy;

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
    pull: &["pull"],
    push: &["push"],
};

pub const REPO_TYPES: &[RepoDefinition] = &[GIT];

// =============================================================================
// Commit Message Validation
// =============================================================================

static COMMIT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(A|M|P):\s((\(.+\))\s)?(.*)$").unwrap()
});

pub fn valid_url(url_str: &str) -> bool {
    if url_str.starts_with("about:") {
        return true;
    }

    if let Some(scheme_end) = url_str.find("://") {
        let scheme = &url_str[..scheme_end];
        if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }

        let rest = &url_str[scheme_end + 3..];
        if rest.is_empty() {
            return false;
        }

        let host_end = rest.find('/').unwrap_or(rest.len());
        let host = &rest[..host_end];

        if host.is_empty() {
            return false;
        }

        return true;
    }

    false
}

pub fn check_comment(comment: &str, user_changes: bool) -> bool {
    match COMMIT_PATTERN.captures(comment) {
        None => {
            eprintln!("The comment \"{}\" is not in the recognised format.", comment);
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

pub fn check_repo_changes(base_cmd: &[String], repo: &RepoDefinition) -> Option<bool> {
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

/// Get the remote URL for constructing PR link
fn get_remote_url(base_cmd: &[String]) -> Option<String> {
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    
    String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
}

/// Get current branch name
fn get_current_branch(base_cmd: &[String]) -> Option<String> {
    let output = Command::new(&base_cmd[0])
        .args(&base_cmd[1..])
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    
    String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
}

/// Convert git remote URL to GitHub web URL
fn remote_to_github_url(remote: &str) -> Option<String> {
    // Handle SSH: git@github.com:user/repo.git
    if remote.starts_with("git@github.com:") {
        let path = remote.trim_start_matches("git@github.com:")
            .trim_end_matches(".git");
        return Some(format!("https://github.com/{}", path));
    }
    // Handle HTTPS: https://github.com/user/repo.git
    if remote.starts_with("https://github.com/") {
        let url = remote.trim_end_matches(".git");
        return Some(url.to_string());
    }
    None
}

/// Create a pull request branch and return PR URL
pub fn create_pull_request(
    repo: &RepoDefinition,
    base_cmd: &[String],
    message: &str,
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

    println!("\nThe following changes will be included in the PR:");
    print_diff(&diff, no_color);

    // Get current branch (base for PR)
    let base_branch = get_current_branch(base_cmd)
        .unwrap_or_else(|| "master".to_string());
    
    // Create branch name with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let pr_branch = format!("fop-update-{}", timestamp);
    
    println!("\nCreating PR branch '{}'...", pr_branch);
    
    // Create and checkout new branch
    let mut cmd = base_cmd.to_vec();
    cmd.extend(["checkout", "-b", &pr_branch].iter().map(|s| s.to_string()));
    let status = Command::new(&cmd[0]).args(&cmd[1..]).status()?;
    if !status.success() {
        eprintln!("Failed to create branch {}", pr_branch);
        return Ok(None);
    }
    
    // Commit changes
    let mut cmd = base_cmd.to_vec();
    cmd.extend(repo.commit.iter().map(|s| s.to_string()));
    cmd.push(message.to_string());
    let status = Command::new(&cmd[0]).args(&cmd[1..]).status()?;
    if !status.success() {
        eprintln!("Failed to commit changes");
        // Switch back to original branch
        let mut cmd = base_cmd.to_vec();
        cmd.extend(["checkout", &base_branch].iter().map(|s| s.to_string()));
        let _ = Command::new(&cmd[0]).args(&cmd[1..]).status();
        return Ok(None);
    }
    
    // Push branch
    println!("Pushing branch to origin...");
    let mut cmd = base_cmd.to_vec();
    cmd.extend(["push", "-u", "origin", &pr_branch].iter().map(|s| s.to_string()));
    let status = Command::new(&cmd[0]).args(&cmd[1..]).status()?;
    if !status.success() {
        eprintln!("Failed to push branch {}", pr_branch);
        // Switch back to original branch
        let mut cmd = base_cmd.to_vec();
        cmd.extend(["checkout", &base_branch].iter().map(|s| s.to_string()));
        let _ = Command::new(&cmd[0]).args(&cmd[1..]).status();
        return Ok(None);
    }
    
    // Switch back to original branch
    let mut cmd = base_cmd.to_vec();
    cmd.extend(["checkout", &base_branch].iter().map(|s| s.to_string()));
    let _ = Command::new(&cmd[0]).args(&cmd[1..]).status();
    
    // Generate PR URL
    let pr_url = get_remote_url(base_cmd)
        .and_then(|remote| remote_to_github_url(&remote))
        .map(|github_url| format!("{}/compare/{}...{}?expand=1", github_url, base_branch, pr_branch));
    
    if let Some(ref url) = pr_url {
        println!("\n{}", "Pull request branch pushed successfully!".green());
        println!("\nCreate PR at:\n  {}", url.cyan());
    } else {
        println!("\nBranch '{}' pushed. Create PR manually on GitHub.", pr_branch);
    }
    
    Ok(pr_url)
}

// =============================================================================
// Commit Operations
// =============================================================================

pub fn commit_changes(
    repo: &RepoDefinition,
    base_cmd: &[String],
    original_difference: bool,
    no_msg_check: bool,
    no_color: bool,
    no_large_warning: bool,
    git_message: &Option<String>,
) -> io::Result<()> {
    let diff = match get_diff(base_cmd, repo) {
        Some(d) if !d.is_empty() => d,
        _ => {
            println!("\nNo changes have been recorded by the repository.");
            return Ok(());
        }
    };

    println!("\nThe following changes have been recorded by the repository:");
    print_diff(&diff, no_color);

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
        
        println!("Committing with message: {}", message);
        
        let mut cmd = base_cmd.to_vec();
        cmd.extend(repo.commit.iter().map(|s| s.to_string()));
        cmd.push(message.clone());
        
        Command::new(&cmd[0]).args(&cmd[1..]).status()?;
        
        // Pull and push
        for op in [repo.pull, repo.push].iter() {
            let mut cmd = base_cmd.to_vec();
            cmd.extend(op.iter().map(|s| s.to_string()));
            let _ = Command::new(&cmd[0]).args(&cmd[1..]).status();
        }
        
        println!("Completed commit process successfully.");
        return Ok(());
    }

    // Check for large changes
    if !no_large_warning && !original_difference && is_large_change(&diff) {
        println!("\nThis is a large change. Are you sure you want to proceed?");
        print!("Please type 'YES' to continue: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim() != "YES" {
            println!("Commit aborted.");
            return Ok(());
        }
    }

    // Get commit comment
    loop {
        print!("Please enter a valid commit comment or quit:\n");
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

        if no_msg_check || check_comment(comment, original_difference) {
            println!("Comment \"{}\" accepted.", comment);

            // Execute commit
            let mut cmd = base_cmd.to_vec();
            cmd.extend(repo.commit.iter().map(|s| s.to_string()));
            cmd.push(comment.to_string());

            let status = Command::new(&cmd[0])
                .args(&cmd[1..])
                .status();

            if let Err(e) = status {
                eprintln!("Unexpected error with commit: {}", e);
                return Err(e);
            }

            // Pull and push
            println!("\nConnecting to server. Please enter your password if required.");

            for op in [repo.pull, repo.push].iter() {
                let mut cmd = base_cmd.to_vec();
                cmd.extend(op.iter().map(|s| s.to_string()));

                let _ = Command::new(&cmd[0])
                    .args(&cmd[1..])
                    .status();
                println!();
            }

            println!("Completed commit process successfully.");
            return Ok(());
        }
        println!();
    }
}