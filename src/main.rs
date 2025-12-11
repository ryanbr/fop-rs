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

mod fop_sort;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use ahash::AHashSet as HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use colored::Colorize;

use once_cell::sync::Lazy;
use regex::Regex;
use walkdir::WalkDir;
use rayon::prelude::*;

use fop_sort::fop_sort;

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
    /// Sort localhost/hosts file entries (0.0.0.0/127.0.0.1)
    localhost: bool,
    /// Disable colored output
    no_color: bool,
    /// Additional files to ignore (comma-separated, supports partial names)
    ignore_files: Vec<String>,
    /// Additional directories to ignore (comma-separated, supports partial names)
    ignore_dirs: Vec<String>,
    /// Disable large change warning prompt
    no_large_warning: bool,
    /// Git commit message (skip interactive prompt)
    git_message: Option<String>,
    /// Show applied configuration
    show_config: bool,
    /// Show help
    help: bool,
    /// Show version
    version: bool,
}

/// Load configuration from .fopconfig file
fn load_config(custom_path: Option<&PathBuf>) -> (HashMap<String, String>, Option<PathBuf>) {
    let mut config = HashMap::new();
    
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
        dirs::home_dir().map(|h| h.join(".fopconfig")).unwrap_or_default(),
    ];
        config_paths.into_iter().find(|p| p.exists())
    };
    
    if let Some(path) = config_path.as_ref() {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                // Skip comments and empty lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                // Parse key = value
                if let Some(eq_pos) = line.find('=') {
                    let key = line[..eq_pos].trim().to_string();
                    let value = line[eq_pos + 1..].trim().to_string();
                    config.insert(key, value);
                }
            }
        }
    }
    
    (config, config_path)
}

/// Parse boolean value from config
fn parse_bool(config: &HashMap<String, String>, key: &str, default: bool) -> bool {
    config.get(key).map(|v| {
        matches!(v.to_lowercase().as_str(), "true" | "yes" | "1")
    }).unwrap_or(default)
}

/// Parse string list from config (comma-separated)
fn parse_list(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    config.get(key).map(|v| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }).unwrap_or_default()
}

impl Args {
    fn parse() -> (Self, Option<String>) {
        // First pass: look for --config-file argument
        let mut config_file: Option<PathBuf> = None;
        for arg in env::args().skip(1) {
            if arg.starts_with("--config-file=") {
                let path = arg.trim_start_matches("--config-file=");
                config_file = Some(PathBuf::from(path));
                break;
            }
        }
        
        // Load config file and track path
        let (config, found_config_path) = load_config(config_file.as_ref());
        // Store for --show-config
        let config_path_str = found_config_path.as_ref().map(|p| p.display().to_string());
        
        // Start with config values (or defaults)
        let mut args = Args {
            directories: Vec::new(),
            no_commit: parse_bool(&config, "no-commit", false),
            no_ubo_convert: parse_bool(&config, "no-ubo-convert", false),
            no_msg_check: parse_bool(&config, "no-msg-check", false),
            disable_ignored: parse_bool(&config, "disable-ignored", false),
            no_sort: parse_bool(&config, "no-sort", false),
            alt_sort: parse_bool(&config, "alt-sort", false),
            localhost: parse_bool(&config, "localhost", false),
            no_color: parse_bool(&config, "no-color", false),
            ignore_files: parse_list(&config, "ignorefiles"),
            ignore_dirs: parse_list(&config, "ignoredirs"),
            git_message: None,
            show_config: false,
            no_large_warning: parse_bool(&config, "no-large-warning", false),
            help: false,
            version: false,
        };
        
        // Command line args override config
        for arg in env::args().skip(1) {
            match arg.as_str() {
                "-h" | "--help" => args.help = true,
                "-V" | "--version" => args.version = true,
                "-n" | "--no-commit" | "--just-sort" | "--justsort" => args.no_commit = true,
                "--no-ubo-convert" => args.no_ubo_convert = true,
                "--no-msg-check" => args.no_msg_check = true,
                "--disable-ignored" => args.disable_ignored = true,
                "--no-sort" => args.no_sort = true,
                "--alt-sort" => args.alt_sort = true,
                "--localhost" => args.localhost = true,
                "--no-color" => args.no_color = true,
                "--no-large-warning" => args.no_large_warning = true,
                "--show-config" => args.show_config = true,
                _ if arg.starts_with("--ignorefiles=") => {
                    let files = arg.trim_start_matches("--ignorefiles=");
                    args.ignore_files = files.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ if arg.starts_with("--config-file=") => {
                    // Already handled in first pass
                }
                _ if arg.starts_with("--ignoredirs=") => {
                    args.ignore_dirs = arg.trim_start_matches("--ignoredirs=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ if arg.starts_with("--git-message=") => {
                    args.git_message = Some(arg.trim_start_matches("--git-message=").to_string());
                }
                _ if arg.starts_with('-') => {
                    eprintln!("Unknown option: {}", arg);
                    eprintln!("Use --help for usage information");
                    std::process::exit(1);
                }
                _ => args.directories.push(PathBuf::from(arg)),
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
        println!("        --localhost     Sort hosts file entries (0.0.0.0/127.0.0.1 domain)");
        println!("        --no-color      Disable colored output");
        println!("        --no-large-warning  Disable large change warning prompt");
        println!("        --ignorefiles=  Additional files to ignore (comma-separated, partial names)");
        println!("        --ignoredirs=   Additional directories to ignore (comma-separated, partial names)");
        println!("        --config-file=  Custom config file path");
        println!("        --git-message=  Git commit message (skip interactive prompt)");
        println!("        --show-config   Show applied configuration and exit");
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
        println!();
        println!("    # Example .fopconfig");
        println!("    no-commit = true");
        println!("    no-ubo-convert = false");
        println!("    ignorefiles = .json,.backup,test.txt");
        println!("    ignoredirs = backup,test");
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
        println!("  no-ubo-convert  = {}", self.no_ubo_convert);
        println!("  no-msg-check    = {}", self.no_msg_check);
        println!("  disable-ignored = {}", self.disable_ignored);
        println!("  no-sort         = {}", self.no_sort);
        println!("  alt-sort        = {}", self.alt_sort);
        println!("  localhost       = {}", self.localhost);
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
pub(crate) static FILTER_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\$(?:[^,]*,)?domain=([^,]+)").unwrap()
});

/// Pattern for extracting domain from element hiding rules  
pub(crate) static ELEMENT_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)#[@?$%]?#"#).unwrap()
});

/// Pattern for FOP.py compatible element matching (no {} in selector)
pub(crate) static FOPPY_ELEMENT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)(#[@?]?#)([^{}]+)$"#).unwrap()
});

/// Pattern for FOP.py compatible sorting (only ## and #@#)
pub(crate) static FOPPY_ELEMENT_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^[^/|@"!]*?#@?#"#).unwrap()
});

/// Pattern for element hiding rules (standard, uBO, and AdGuard extended syntax)
pub(crate) static ELEMENT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)(##|#@#|#\?#|#@\?#|#\$#|#@\$#|#%#|#@%#)(.+)$"#).unwrap()
});

/// Pattern for regex domain element hiding rules (uBO/AdGuard specific)
pub(crate) static REGEX_ELEMENT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^(/[^#]+/)(##|#@#|#\?#|#@\?#|#\$#|#@\$#|#%#|#@%#)(.+)$"#).unwrap()
});

/// Pattern for localhost/hosts file entries
pub(crate) static LOCALHOST_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(0\.0\.0\.0|127\.0\.0\.1)\s+(.+)$").unwrap()
});

pub(crate) static OPTION_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(.*)\$(~?[\w\-]+(?:=[^,\s]+)?(?:,~?[\w\-]+(?:=[^,\s]+)?)*)$").unwrap()
});

pub(crate) static PSEUDO_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(:[a-zA-Z\-]*[A-Z][a-zA-Z\-]*)").unwrap()
});

pub(crate) static REMOVAL_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"([>+~,@\s])(\*)([#.\[:])")
        .expect("Invalid REMOVAL_PATTERN regex")
});

pub(crate) static ATTRIBUTE_VALUE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^'"\\]|\\.)*("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|\*"#).unwrap()
});

pub(crate) static TREE_SELECTOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\\.|[^+>~ \t])\s*([+>~ \t])\s*(\D)").unwrap()
});

pub(crate) static UNICODE_SELECTOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\\[0-9a-fA-F]{1,6}\s[a-zA-Z]*[A-Z]").unwrap()
});

pub(crate) static TLD_ONLY_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\|\||[|])?\.([a-z]{2,})\^?$").unwrap()
});

static COMMIT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(A|M|P):\s(\((.+)\)\s)?(.*)$").unwrap()
});

pub(crate) static SHORT_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\|*[a-zA-Z0-9]").unwrap()
});

pub(crate) static DOMAIN_EXTRACT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\|*([^/\^\$]+)").unwrap()
});

pub(crate) static IP_ADDRESS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\d+\.\d+\.\d+\.\d+").unwrap()
});

// =============================================================================
// Constants
// =============================================================================

/// Files that should not be sorted
const IGNORE_FILES: &[&str] = &[
    "test-files-to-ingore.txt",
];

/// Directories to ignore
const IGNORE_DIRS: &[&str] = &[
    "folders-to-ingore",
];

/// Domains that should ignore the 7 character size restriction
pub(crate) static IGNORE_DOMAINS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    set.insert("a.sampl");
    set
});

/// Known Adblock Plus options (HashSet for O(1) lookup)
pub(crate) static KNOWN_OPTIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        // Standard ABP options
        "collapse", "csp", "csp=frame-src", "csp=img-src", "csp=media-src",
        "csp=script-src", "csp=worker-src", "document", "elemhide", "font",
        "genericblock", "generichide", "image", "match-case", "media",
        "object-subrequest", "object", "other", "ping", "popup",
        "script", "stylesheet", "subdocument", "third-party", "webrtc",
        "websocket", "xmlhttprequest",
        // uBO short options
        "xhr", "css", "1p", "3p", "frame", "doc", "ghide",
        // uBO/ABP specific
        "all", "badfilter", "important", "popunder",
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
    ].into_iter().collect()
});

/// uBO to ABP option conversions
pub(crate) static UBO_CONVERSIONS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
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
    ].into_iter().collect()
});

// =============================================================================
// Repository Types
// =============================================================================

#[derive(Clone)]
struct RepoDefinition {
    name: &'static str,
    directory: &'static str,
    location_option: &'static str,
    repo_directory_option: Option<&'static str>,
    check_changes: &'static [&'static str],
    difference: &'static [&'static str],
    commit: &'static [&'static str],
    pull: &'static [&'static str],
    push: &'static [&'static str],
}

const GIT: RepoDefinition = RepoDefinition {
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

const REPO_TYPES: &[RepoDefinition] = &[GIT];

// =============================================================================
// Repository Functions
// =============================================================================

fn build_base_command(repo: &RepoDefinition, location: &Path) -> Vec<String> {
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

fn check_repo_changes(base_cmd: &[String], repo: &RepoDefinition) -> Option<bool> {
    let mut cmd = base_cmd.to_vec();
    cmd.extend(repo.check_changes.iter().map(|s| s.to_string()));

    let output = Command::new(&cmd[0])
        .args(&cmd[1..])
        .output()
        .ok()?;

    Some(!output.stdout.is_empty())
}

fn get_diff(base_cmd: &[String], repo: &RepoDefinition) -> Option<String> {
    let mut cmd = base_cmd.to_vec();
    cmd.extend(repo.difference.iter().map(|s| s.to_string()));

    let output = Command::new(&cmd[0])
        .args(&cmd[1..])
        .output()
        .ok()?;

    String::from_utf8(output.stdout).ok()
}

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

pub(crate) fn valid_url(url_str: &str) -> bool {
    // Handle about: URLs specially
    if url_str.starts_with("about:") {
        return true;
    }

    // Simple URL validation: check for scheme://host/path pattern
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

pub(crate) fn check_comment(comment: &str, user_changes: bool) -> bool {
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

fn commit_changes(
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

// =============================================================================
// Main Processing
// =============================================================================

/// Check if filename matches any ignore pattern (exact or partial)
fn should_ignore_file(filename: &str, ignore_files: &[String]) -> bool {
    for pattern in ignore_files {
        if filename == pattern || filename.contains(pattern) {
            return true;
        }
    }
    false
}

/// Check if directory path matches any ignore pattern
fn should_ignore_dir(path: &Path, ignore_dirs: &[String]) -> bool {
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            for pattern in ignore_dirs {
                if name == pattern || name.contains(pattern) {
                    return true;
                }
            }
        }
    }
    false
}

fn process_location(location: &Path, no_commit: bool, convert_ubo: bool, no_msg_check: bool, disable_ignored: bool, no_sort: bool, alt_sort: bool, localhost: bool, no_color: bool, no_large_warning: bool, ignore_files: &[String], ignore_dirs: &[String], git_message: &Option<String>) -> io::Result<()> {
    if !location.is_dir() {
        eprintln!("{} does not exist or is not a folder.", location.display());
        return Ok(());
    }

    // Detect repository type (skip if no_commit mode)
    let mut repository: Option<&RepoDefinition> = None;
    if !no_commit {
        for repo_type in REPO_TYPES {
            if location.join(repo_type.directory).is_dir() {
                repository = Some(repo_type);
                break;
            }
        }
    }

    // Check initial repository state
    let (base_cmd, original_difference) = if let Some(repo) = repository {
        let base_cmd = build_base_command(repo, location);
        match check_repo_changes(&base_cmd, repo) {
            Some(diff) => (Some(base_cmd), diff),
            None => {
                eprintln!(
                    "The repository command was unable to run; FOP will not attempt to use repository tools."
                );
                (None, false)
            }
        }
    } else {
        (None, false)
    };

    println!("\nPrimary location: {}", location.display());

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
        if path.is_dir() {
            println!("Current directory: {}", path.display());
        }
    }

    // Collect text files to process
    let txt_files: Vec<_> = entries
        .iter()
        .filter(|entry| {
            let path = entry.path();
            if path.is_dir() {
                return false;
            }
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            extension == "txt" 
                && (disable_ignored || !IGNORE_FILES.contains(&filename))
                && !should_ignore_file(filename, ignore_files)
        })
        .collect();

    // Process files in parallel
    txt_files.par_iter().for_each(|entry| {
        if let Err(e) = fop_sort(entry.path(), convert_ubo, no_sort, alt_sort, localhost) {
            eprintln!("Error processing {}: {}", entry.path().display(), e);
        }
    });

    // Delete backup and temp files (sequential, usually few files)
    for entry in &entries {
        let path = entry.path();
        if path.is_file() {
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if extension == "orig" || extension == "temp" {
                let _ = fs::remove_file(path);
            }
        }
    }

    // Offer to commit changes (skip if no_commit mode)
    if !no_commit {
        if let (Some(repo), Some(base_cmd)) = (repository, base_cmd) {
            commit_changes(repo, &base_cmd, original_difference, no_msg_check, no_color, no_large_warning, git_message)?;
        }
    }

    Ok(())
}

fn print_greeting(no_commit: bool, config_path: Option<&str>) {
    let mode = if no_commit { " (sort only)" } else { "" };
    let greeting = format!("FOP (Filter Orderer and Preener) version {}{}", VERSION, mode);
    let separator = "=".repeat(greeting.len());
    println!("{}", separator);
    println!("{}", greeting);
    println!("Copyright (C) 2025 FanboyNZ - https://github.com/ryanbr/fop-rs (Licensed under GPL-3.0)");
    if let Some(path) = config_path {
        println!("Using config file: {}", path);
    }
    println!("{}", separator);
}

fn main() {
    let (args, config_path) = Args::parse();

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

    print_greeting(args.no_commit, config_path.as_deref());

    if args.directories.is_empty() {
        // Process current directory
        if let Ok(cwd) = env::current_dir() {
            if let Err(e) = process_location(&cwd, args.no_commit, !args.no_ubo_convert, args.no_msg_check, args.disable_ignored, args.no_sort, args.alt_sort, args.localhost, args.no_color, args.no_large_warning, &args.ignore_files, &args.ignore_dirs, &args.git_message) {
                eprintln!("Error: {}", e);
            }
        }
    } else {
        // Process specified directories
        let mut unique_places: Vec<PathBuf> = args.directories
            .iter()
            .filter_map(|p| fs::canonicalize(p).ok())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        unique_places.sort();

        for place in unique_places {
            if let Err(e) = process_location(&place, args.no_commit, !args.no_ubo_convert, args.no_msg_check, args.disable_ignored, args.no_sort, args.alt_sort, args.localhost, args.no_color, args.no_large_warning, &args.ignore_files, &args.ignore_dirs, &args.git_message) {
                eprintln!("Error: {}", e);
            }
            println!();
        }
    }
}
