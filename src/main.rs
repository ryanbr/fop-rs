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

mod fop_git;
mod fop_checksum;
mod fop_sort;
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

/// Get user's home directory (cross-platform)
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
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

/// Write warning to buffer (if file output) or stderr
pub(crate) fn write_warning(message: &str) {
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
    
    use std::fs::OpenOptions;
    use std::io::{BufWriter, Write};
    if let Ok(file) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
    {
        let mut writer = BufWriter::new(file);
        for msg in warnings {
            let _ = writeln!(writer, "{}", msg);
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
    /// Don't skip rules without dot in domain
    ignore_dot_domains: bool,
    /// Output warnings to file instead of stderr
    warning_output: Option<PathBuf>,
    /// Create PR branch instead of committing to master (optional: PR title)
    create_pr: Option<String>,
    /// Fix cosmetic typos in all processed files
    fix_typos: bool,
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
}

/// Load configuration from .fopconfig file
fn load_config(custom_path: Option<&PathBuf>) -> (HashMap<String, String>, Option<PathBuf>) {
    // pre-allocated config settings
    let mut config = HashMap::with_capacity(28);

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
        let (config, found_config_path) = if ignore_config {
            (HashMap::new(), None)
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
            warning_output: config.get("warning-output").map(PathBuf::from),
            create_pr: config.get("create-pr").and_then(|v| {
                match v.to_lowercase().as_str() {
                    "" | "true" | "yes" | "1" => Some(String::new()), // Enable with prompt
                    "false" | "no" | "0" => None,                     // Disable
                    _ => Some(v.clone()),                             // Use as title
                }
            }),
            git_pr_branch: config.get("git-pr-branch").cloned(),
            pr_show_changes: parse_bool(&config, "pr-show-changes", false),
            check_banned_list: config.get("check-banned-list").map(PathBuf::from),
            auto_banned_remove: parse_bool(&config, "auto-banned-remove", false),
            fix_typos: parse_bool(&config, "fix-typos", false),
            fix_typos_on_add: parse_bool(&config, "fix-typos-on-add", false),
            direct_push_users: config.get("direct-push-users")
                .map(|s| s.split(',').map(|u| u.trim().to_lowercase()).collect())
                .unwrap_or_default(),
            quiet: parse_bool(&config, "quiet", false),
            limited_quiet: parse_bool(&config, "limited-quiet", false),
            auto_fix: parse_bool(&config, "auto-fix", false),
            output_diff: config.get("output-diff").map(PathBuf::from),
            output_diff_individual: false,
            check_file: None,
            output_changed: false,
            only_sort_changed: parse_bool(&config, "only-sort-changed", false),
            rebase_on_fail: parse_bool(&config, "rebase-on-fail", false),
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
        };

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
                "--pr-show-changes" => args.pr_show_changes = true,
                _ if arg.starts_with("--check-banned-list=") => {
                    args.check_banned_list = Some(PathBuf::from(arg.trim_start_matches("--check-banned-list=")));
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
                    args.warning_output =
                        Some(PathBuf::from(arg.trim_start_matches("--warning-output=")));
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
                "--fix-typos-on-add" => args.fix_typos_on_add = true,
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
                    args.check_file = Some(PathBuf::from(arg.trim_start_matches("--check-file=")));
                }
                "--quiet" | "-q" => args.quiet = true,
                "--limited-quiet" => args.limited_quiet = true,
                "--ci" => args.ci = true,
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
                        Some(PathBuf::from(arg.trim_start_matches("--output-diff=")));
                }
                _ if arg.starts_with("--git-message=") => {
                    args.git_message = Some(arg.trim_start_matches("--git-message=").to_string());
                }
                _ if arg.starts_with("--add-timestamp=") => {
                    args.add_timestamp = arg.trim_start_matches("--add-timestamp=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
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
        println!("        --no-large-warning  Disable large change warning prompt");
        println!("        --ignorefiles=  Additional files to ignore (comma-separated, partial names)");
        println!("        --ignoredirs=   Additional directories to ignore (comma-separated, partial names)");
        println!("        --ignore-all-but=   Only process these files, ignore all others (comma-separated)");
        println!("        --config-file=  Custom config file path");
        println!("        --file-extensions=  File extensions to process (default: .txt)");
        println!("        --comments=     Comment line prefixes (default: !)");
        println!("        --backup        Create .backup files before modifying");
        println!("        --keep-empty-lines  Keep empty lines in output");
        println!("        --ignore-dot-domains  Don't skip rules without dot in domain");
        println!("        --warning-output=   Output warnings to file instead of stderr");
        println!("        --git-message=  Git commit message (skip interactive prompt)");
        println!("        --create-pr[=TITLE]  Create PR branch instead of committing to master");
        println!("        --git-pr-branch=NAME   Base branch for PR (default: main/master)");
        println!("        --fix-typos      Fix cosmetic rule typos in all files");
        println!("        --fix-typos-on-add   Check cosmetic rule typos in git additions");
        println!("        --auto-fix           Auto-fix typos without prompting");
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
    Regex::new(r"^(.*)\$(~?[\w\-]+(?:=[^,\s]+)?(?:,~?[\w\-]+(?:=[^,\s]+)?)*)$").unwrap()
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

pub(crate) static IP_ADDRESS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.\d+\.\d+\.\d+").unwrap());

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
fn entry_is_file(entry: &DirEntry) -> bool {
    let ft = entry.file_type();
    ft.is_file() || (ft.is_symlink() && entry.path().is_file())
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
    auto_fix: bool,
    only_sort_changed: bool,
    rebase_on_fail: bool,
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
) -> io::Result<()> {
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
        let base_cmd = build_base_command(repo, location, git_binary);
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
    let txt_files: Vec<_> = entries
        .iter()
        .filter(|entry| {
            let path = entry.path();
            if entry_is_dir(entry) {
                return false;
            }
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            file_extensions.iter().any(|ext| ext == extension)
                && (disable_ignored || !IGNORE_FILES.contains(&filename))
                && !should_ignore_file(filename, ignore_files)
                && (ignore_all_but.is_empty()
                    || ignore_all_but.iter().any(|f| filename.contains(f)))
        })
        .collect();

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
        let config = SortConfig {
            convert_ubo: sort_config.convert_ubo,
            no_sort: sort_config.no_sort,
            alt_sort: sort_config.alt_sort,
            parse_adguard: sort_config.parse_adguard || {
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                parse_adguard_files.iter().any(|f| fname == f.as_str())
                    || parse_adguard_files.iter().any(|f| path.ends_with(f.as_str()))
            },
            localhost: sort_config.localhost || {
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                localhost_files.iter().any(|f| fname == f.as_str())
                    || localhost_files.iter().any(|f| path.ends_with(f.as_str()))
            },
            comment_chars: sort_config.comment_chars,
            backup: sort_config.backup,
            keep_empty_lines: sort_config.keep_empty_lines,
            ignore_dot_domains: sort_config.ignore_dot_domains,
            fix_typos,
            quiet,
            no_color,
            dry_run: sort_config.dry_run,
            output_changed: sort_config.output_changed,
            add_timestamp: sort_config.add_timestamp,
        };

        match fop_sort(path, &config) {
            Ok(Some(diff)) => {
                if output_diff_individual {
                    // Individual mode: write .diff file alongside source
                    let diff_path = entry.path().with_extension("diff");
                    if let Err(e) = fs::write(&diff_path, &diff) {
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

    // Delete backup and temp files (sequential, usually few files)
    for entry in &entries {
        let path = entry.path();
        if entry_is_file(entry) {
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if extension == "orig" || extension == "temp" {
                let _ = fs::remove_file(path);
            }
        }
    }

    // Add timestamps to specified files (after sorting, before checksum)
    if !add_timestamp.is_empty() {
        for entry in &entries {
            if entry_is_file(entry) {
                let path = entry.path();
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if add_timestamp.iter().any(|f| filename == f.as_str())
                    || add_timestamp.iter().any(|f| path.ends_with(f.as_str()))
                {
                    let is_localhost = localhost || {
                        localhost_files.iter().any(|f| filename == f.as_str())
                            || localhost_files.iter().any(|f| path.ends_with(f.as_str()))
                    };
                    let _ = fop_datestamp::add_timestamp(path, is_localhost, quiet, no_color);
                }
            }
        }
    }

    // Add checksums to specified files (after sorting, before commit)
    if !add_checksum.is_empty() {
        for entry in &entries {
            if entry_is_file(entry) {
                let path = entry.path();
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if add_checksum.iter().any(|f| filename == f.as_str())
                    || add_checksum.iter().any(|f| path.ends_with(f.as_str()))
                {
                    let is_localhost = localhost || {
                        localhost_files.iter().any(|f| filename == f.as_str())
                            || localhost_files.iter().any(|f| path.ends_with(f.as_str()))
                    };
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
    if !validate_checksum_and_fix.is_empty() {
        for entry in &entries {
            if entry_is_file(entry) {
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
                            let is_localhost = localhost || {
                                localhost_files.iter().any(|f| filename == f.as_str())
                                    || localhost_files.iter().any(|f| path.ends_with(f.as_str()))
                            };
                            if let Err(e) = fop_checksum::add_checksum(path, is_localhost, quiet, no_color) {
                                eprintln!("Error fixing checksum for {}: {}", path.display(), e);
                            }
                        }
                        Ok(fop_checksum::ChecksumResult::Missing) => {
                            if !quiet {
                                eprintln!("Checksum MISSING: {} - adding...", path.display());
                            }
                            let is_localhost = localhost || {
                                localhost_files.iter().any(|f| filename == f.as_str())
                                    || localhost_files.iter().any(|f| path.ends_with(f.as_str()))
                            };
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

            // Check for typos in added lines
            // Get added lines once for both typo and banned domain checks
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
                    commit_changes(repo, &base_cmd, original_difference, no_msg_check, no_color, no_large_warning, quiet, limited_quiet, rebase_on_fail, git_message, history)?;
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

fn main() {
    let (mut args, config_path) = Args::parse();

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
        *WARNING_OUTPUT.lock().unwrap() = Some(path.clone());
        WARNING_TO_FILE.store(true, std::sync::atomic::Ordering::Relaxed);
        // Clear existing file
        let _ = std::fs::write(path, "");
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
        fix_typos: args.fix_typos,
        quiet: args.quiet,
        no_color: args.no_color,
        dry_run: args.output_diff.is_some() || args.output_diff_individual || args.output_changed,
        output_changed: args.output_changed,
        add_timestamp: !args.add_timestamp.is_empty(),
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
    if let Some(banned) = args.ci.then_some(()).and(banned_domains.as_ref()) {
        let mut found: Vec<(String, String)> = Vec::new();
        let mut current_file = String::new();

        let base = if std::process::Command::new("git")
            .args(["diff", "--quiet", "HEAD", "origin/master"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        { "HEAD~1" } else { "origin/master" };

        if let Ok(output) = std::process::Command::new("git").args(["diff", base, "--unified=0"]).output() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some(file) = line.strip_prefix("+++ b/") {
                    current_file = file.to_string();
                    continue;
                }
                if current_file.is_empty() || args.ignore_files.iter().any(|f| current_file.ends_with(f)) { continue; }
                if line.starts_with('+') && !line.starts_with("+++") {
                    if let Some(domain) = fop_sort::check_banned_domain(&line[1..], banned) {
                        found.push((domain, line[1..].to_string()));
                    }
                }
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
                    if !e.path().is_file() {
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

        let check_file_config = SortConfig {
            localhost: sort_config.localhost || {
                let fname = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                args.localhost_files.iter().any(|f| fname == f.as_str())
                    || args.localhost_files.iter().any(|f| file_path.ends_with(f.as_str()))
            },
            parse_adguard: sort_config.parse_adguard || {
                let fname = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                args.parse_adguard_files.iter().any(|f| fname == f.as_str())
                    || args.parse_adguard_files.iter().any(|f| file_path.ends_with(f.as_str()))
            },
            ..sort_config
        };
        match fop_sort::fop_sort(file_path, &check_file_config) {
            Ok(Some(diff)) => {
                if args.output_diff_individual {
                    // Individual mode: write .diff file alongside source
                    let diff_path = file_path.with_extension("diff");
                    if let Err(e) = fs::write(&diff_path, &diff) {
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

        // Add checksum if requested
        if !args.add_checksum.is_empty() {
            let filename = file_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
                if args.add_checksum.iter().any(|f| filename == f.as_str())
                    || args.add_checksum.iter().any(|f| file_path.ends_with(f.as_str()))
                {
                let is_localhost = args.localhost || {
                    let fname = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    args.localhost_files.iter().any(|f| fname == f.as_str())
                        || args.localhost_files.iter().any(|f| file_path.ends_with(f.as_str()))
                };
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
                ) {
                    eprintln!("Git error: {}", e);
                }
            }
        }

        // Write diff if requested
        if let Some(ref diff_path) = &args.output_diff {
            let diffs = diff_output.lock().unwrap();
            if let Err(e) = fs::write(diff_path, diffs.join("\n")) {
                eprintln!("Error writing diff file: {}", e);
            }
        }
        return;
    }

    // Clear any previous tracking data and enable if needed
    if args.pr_show_changes {
        fop_sort::clear_tracked_changes();
        TRACK_CHANGES.store(true, std::sync::atomic::Ordering::Relaxed);
    }

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
            args.auto_fix,
            args.only_sort_changed,
            args.rebase_on_fail,
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
        ) {
            eprintln!("Error: {}", e);
        }
        // Print blank line between multiple directories (preserve original behavior)
        if locations.len() > 1 && i < locations.len() - 1 {
            println!();
        }
    }

    // Clear tracking data when done (free memory)
    if args.pr_show_changes {
        TRACK_CHANGES.store(false, std::sync::atomic::Ordering::Relaxed);
        fop_sort::clear_tracked_changes();
    }

    // Write collected diffs if --output-diff specified
    if let Some(ref diff_path) = &args.output_diff {
        let diffs = diff_output.lock().unwrap();
        if let Err(e) = fs::write(diff_path, diffs.join("\n")) {
            eprintln!("Error writing diff file: {}", e);
        } else if !args.quiet && !diffs.is_empty() {
            println!("Diff written to: {}", diff_path.display());
        }
    }

    // Flush any buffered warnings to file
    flush_warnings();
}
