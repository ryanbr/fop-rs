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
mod fop_git;
mod fop_typos;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use ahash::AHashMap;
use ahash::AHashSet as HashSet;
use std::sync::Mutex;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
/// Thread-safe warning output
pub(crate) static WARNING_BUFFER: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::with_capacity(100)));
pub(crate) static WARNING_OUTPUT: Lazy<Mutex<Option<PathBuf>>> = Lazy::new(|| Mutex::new(None));

/// Write warning to buffer (if file output) or stderr
pub(crate) fn write_warning(message: &str) {
    let guard = WARNING_OUTPUT.lock().unwrap();
    if guard.is_some() {
        drop(guard);  // Release lock before acquiring buffer lock
        if let Ok(mut buffer) = WARNING_BUFFER.lock() {
            buffer.push(message.to_string());
        }
    } else {
        eprintln!("{}", message);
    }
}

/// Flush buffered warnings to file
pub(crate) fn flush_warnings() {
    let path_guard = WARNING_OUTPUT.lock().unwrap();
    if let Some(ref path) = *path_guard {
        let mut buffer = WARNING_BUFFER.lock().unwrap();
        if !buffer.is_empty() {
            use std::fs::OpenOptions;
            use std::io::{BufWriter, Write};
            if let Ok(file) = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
            {
                let mut writer = BufWriter::new(file);
                for msg in buffer.drain(..) {
                    let _ = writeln!(writer, "{}", msg);
                }
            }
        }
    }
}

use regex::Regex;
use walkdir::WalkDir;
use rayon::prelude::*;

use fop_sort::{fop_sort, SortConfig};
use fop_git::{RepoDefinition, REPO_TYPES, build_base_command, check_repo_changes,
              commit_changes, create_pull_request, git_available, get_added_lines};

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
    /// Files to disable short domain length check (comma-separated)
    disable_domain_limit: Vec<String>,
    /// Output warnings to file instead of stderr
    warning_output: Option<PathBuf>,
    /// Create PR branch instead of committing to master (optional: PR title)
    create_pr: Option<String>,
    /// Fix cosmetic typos in all processed files
    fix_typos: bool,
    /// Base branch for PR (default: auto-detect main/master)
    git_pr_branch: Option<String>,
    /// Check typos in git additions before commit
    fix_typos_on_add: bool,
    /// Auto-fix without prompting (use with --fix-typos or --fix-typos-on-add)
    auto_fix: bool,
    /// Output changes as diff file (no actual changes made)
    output_diff: Option<PathBuf>,
    /// Suppress most output (for CI)
    quiet: bool,
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

/// Normalize extension to exclude leading dot (for path.extension() comparison)
fn normalize_extension(ext: &str) -> String {
    ext.trim_start_matches('.').to_string()
}

/// Parse file extensions from config (comma-separated), default to txt
fn parse_extensions(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    config.get(key).map(|v| {
        v.split(',')
            .map(|s| normalize_extension(s.trim()))
            .filter(|s| !s.is_empty())
            .collect()
    }).unwrap_or_else(|| vec!["txt".to_string()])
}

/// Parse comment characters from config (comma-separated), default to !
fn parse_comment_chars(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    config.get(key).map(|v| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }).unwrap_or_else(|| vec!["!".to_string()])
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
            file_extensions: parse_extensions(&config, "file-extensions"),
            comment_chars: parse_comment_chars(&config, "comments"),
            backup: parse_bool(&config, "backup", false),
            keep_empty_lines: parse_bool(&config, "keep-empty-lines", false),
            ignore_dot_domains: parse_bool(&config, "ignore-dot-domains", false),
            disable_domain_limit: parse_list(&config, "disable-domain-limit"),
            warning_output: config.get("warning-output").map(|s| PathBuf::from(s)),
            create_pr: config.get("create-pr").cloned(),
            git_pr_branch: config.get("git-pr-branch").cloned(),
            fix_typos: parse_bool(&config, "fix-typos", false),
            fix_typos_on_add: parse_bool(&config, "fix-typos-on-add", false),
            quiet: parse_bool(&config, "quiet", false),
            auto_fix: parse_bool(&config, "auto-fix", false),
            output_diff: config.get("output-diff").map(PathBuf::from),
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
                _ if arg.starts_with("--file-extensions=") => {
                    args.file_extensions = arg.trim_start_matches("--file-extensions=")
                        .split(',')
                        .map(|s| normalize_extension(s.trim()))
                         .collect();
                }
                _ if arg.starts_with("--comments=") => {
                    args.comment_chars = arg.trim_start_matches("--comments=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "--backup" => args.backup = true,
                "--keep-empty-lines" => args.keep_empty_lines = true,
                _ if arg.starts_with("--disable-domain-limit=") => {
                    args.disable_domain_limit = arg.trim_start_matches("--disable-domain-limit=")
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ if arg.starts_with("--warning-output=") => {
                    args.warning_output = Some(PathBuf::from(arg.trim_start_matches("--warning-output=")));
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
                "--create-pr" => args.create_pr = Some(String::new()),
                _ if arg.starts_with("--create-pr=") => {
                    args.create_pr = Some(arg.trim_start_matches("--create-pr=").to_string());
                }
                _ if arg.starts_with("--git-pr-branch=") => {
                    args.git_pr_branch = Some(arg.trim_start_matches("--git-pr-branch=").to_string());
                }
                "--fix-typos" => args.fix_typos = true,
                "--fix-typos-on-add" => args.fix_typos_on_add = true,
                "--auto-fix" => args.auto_fix = true,
                "--quiet" | "-q" => args.quiet = true,
                _ if arg.starts_with("--output-diff=") => {
                    args.output_diff = Some(PathBuf::from(arg.trim_start_matches("--output-diff=")));
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
        println!("        --file-extensions=  File extensions to process (default: .txt)");
        println!("        --comments=     Comment line prefixes (default: !)");
        println!("        --backup        Create .backup files before modifying");
        println!("        --keep-empty-lines  Keep empty lines in output");
        println!("        --ignore-dot-domains  Don't skip rules without dot in domain");
        println!("        --disable-domain-limit=  Files to skip short domain check (comma-separated)");
        println!("        --warning-output=   Output warnings to file instead of stderr");
        println!("        --git-message=  Git commit message (skip interactive prompt)");
        println!("        --create-pr[=TITLE]  Create PR branch instead of committing to master");
        println!("        --git-pr-branch=NAME   Base branch for PR (default: main/master)");
        println!("        --fix-typos      Fix cosmetic rule typos in all files");
        println!("        --fix-typos-on-add   Check cosmetic rule typos in git additions");
        println!("        --auto-fix           Auto-fix typos without prompting");
        println!("    -q, --quiet                Suppress most output (for CI)");
        println!("        --output-diff=FILE     Output changes as diff (no files modified)");
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
        if self.file_extensions.is_empty() || (self.file_extensions.len() == 1 && self.file_extensions[0] == "txt") {
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
        if self.disable_domain_limit.is_empty() {
            println!("  disable-domain-limit= (none)");
        } else {
            println!("  disable-domain-limit= {}", self.disable_domain_limit.join(","));
        }
        if let Some(ref path) = self.warning_output {
            println!("  warning-output  = {}", path.display());
        } else {
            println!("  warning-output  = (stderr)");
        }
        if let Some(ref title) = self.create_pr {
            println!("  create-pr       = {}", if title.is_empty() { "(prompt)" } else { title });
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
pub(crate) static FILTER_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\$(?:[^,]*,)?domain=([^,]+)").unwrap()
});

/// Pattern for extracting domain from element hiding rules  
pub(crate) static ELEMENT_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)#[@?$%]?#"#).unwrap()
});

/// Pattern for FOP.py compatible element matching (no {} in selector)
pub(crate) static FOPPY_ELEMENT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)(#[@?$%]?#|#@[$%?]#)([^{}]+)$"#).unwrap()
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
        "xhr", "css", "1p", "3p", "frame", "doc", "ghide", "xml", "iframe",
        "first-party", "strict1p", "strict3p", "ehide", "shide", "specifichide",
        // uBO/ABP specific
        "all", "badfilter", "important", "popunder", "empty", "cname",
        "inline-script", "removeparam", "redirect-rule",
        "_____", "-----", 
        // Adguard
        "network", "content", "extension", "jsinject", "stealth", "cookie",
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
pub(crate) static UBO_CONVERSIONS: Lazy<AHashMap<&'static str, &'static str>> = Lazy::new(|| {
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

fn process_location(
    location: &Path,
    no_commit: bool,
    no_msg_check: bool,
    disable_ignored: bool,
    no_color: bool,
    no_large_warning: bool,
    ignore_files: &[String],
    ignore_dirs: &[String],
    file_extensions: &[String],
    disable_domain_limit: &[String],
    sort_config: &SortConfig,
    create_pr: &Option<String>,
    git_pr_branch: &Option<String>,
    fix_typos: bool,
    fix_typos_on_add: bool,
    auto_fix: bool,
    quiet: bool,
    diff_output: &std::sync::Mutex<Vec<String>>,
    git_message: &Option<String>,
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

    if !quiet {
        println!("\nPrimary location: {}", location.display());
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
        if path.is_dir() {
            if !quiet {
                println!("Current directory: {}", path.display());
            }
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
            file_extensions.iter().any(|ext| ext == extension) 
                && (disable_ignored || !IGNORE_FILES.contains(&filename))
                && !should_ignore_file(filename, ignore_files)
        })
        .collect();

    // Process files in parallel
    txt_files.par_iter().for_each(|entry| {
        let filename = entry.path().file_name().and_then(|n| n.to_str()).unwrap_or("");
        let skip_domain_limit = disable_domain_limit.iter().any(|f| filename.contains(f));
        let config = SortConfig {
            convert_ubo: sort_config.convert_ubo,
            no_sort: sort_config.no_sort,
            alt_sort: sort_config.alt_sort,
            localhost: sort_config.localhost,
            comment_chars: sort_config.comment_chars,
            backup: sort_config.backup,
            keep_empty_lines: sort_config.keep_empty_lines,
            ignore_dot_domains: sort_config.ignore_dot_domains,
            disable_domain_limit: skip_domain_limit,
            fix_typos,
            quiet,
            dry_run: sort_config.dry_run,
        };

        match fop_sort(entry.path(), &config) {
            Ok(Some(diff)) => {
                // Collect diff for dry-run mode
                diff_output.lock().unwrap().push(diff);
            }
            Ok(None) => {}
            Err(e) => eprintln!("Error processing {}: {}", entry.path().display(), e),
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
            if !git_available() {
                eprintln!("Error: git not found in PATH");
                return Ok(());
            }

            // Check for typos in added lines
            if fix_typos_on_add {
                if let Some(additions) = get_added_lines(&base_cmd) {
                    let typos = fop_typos::check_additions(&additions);
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

            if let Some(pr_title) = create_pr {
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
                create_pull_request(repo, &base_cmd, &message, git_pr_branch, quiet, no_color)?;
            } else {
                commit_changes(repo, &base_cmd, original_difference, no_msg_check, no_color, no_large_warning, quiet, git_message)?;
        }
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

    if !args.quiet {
        print_greeting(args.no_commit, config_path.as_deref());
    }

    // Set warning output path
    if let Some(ref path) = args.warning_output {
        *WARNING_OUTPUT.lock().unwrap() = Some(path.clone());
        // Clear existing file
        let _ = std::fs::write(path, "");
    }

    // Build sort config
    let sort_config = SortConfig {
        convert_ubo: !args.no_ubo_convert,
        no_sort: args.no_sort,
        alt_sort: args.alt_sort,
        localhost: args.localhost,
        comment_chars: &args.comment_chars,
        backup: args.backup,
        keep_empty_lines: args.keep_empty_lines,
        ignore_dot_domains: args.ignore_dot_domains,
        disable_domain_limit: false,  // Set per-file in process_location
        fix_typos: args.fix_typos,
        quiet: args.quiet,
        dry_run: args.output_diff.is_some()
    };

    let diff_output: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    // Build list of locations to process
    let locations: Vec<PathBuf> = if args.directories.is_empty() {
        env::current_dir().map(|cwd| vec![cwd]).unwrap_or_default()
    } else {
        let mut unique: Vec<PathBuf> = args.directories
            .iter()
            .filter_map(|p| fs::canonicalize(p).ok())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        unique.sort();
        unique
    };

    use std::sync::atomic::{AtomicUsize, Ordering};
    use rayon::prelude::*;

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
                    let ext = e.path().extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("");
                    let filename = e.path().file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
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
                    let mut new_lines = Vec::with_capacity(content.lines().count());
                    
                    for (line_num, line) in content.lines().enumerate() {
                        let (fixed, fixes) = fop_typos::fix_all_typos(line);
                        if !fixes.is_empty() {
                            file_typo_count += 1;
                            file_modified = true;
                            if !args.quiet {
                                println!("{}:{}: {} ? {} ({})", 
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
                        if let Err(e) = fs::write(path, new_lines.join("\n") + "\n") {
                            eprintln!("Error writing {}: {}", path.display(), e);
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

    // Process all locations
    for (i, location) in locations.iter().enumerate() {
        if let Err(e) = process_location(location, args.no_commit, args.no_msg_check, args.disable_ignored, args.no_color, args.no_large_warning, &args.ignore_files, &args.ignore_dirs, &args.file_extensions, &args.disable_domain_limit, &sort_config, &args.create_pr, &args.git_pr_branch, args.fix_typos, args.fix_typos_on_add, args.auto_fix, args.quiet, &diff_output, &args.git_message) {
            eprintln!("Error: {}", e);
        }
        // Print blank line between multiple directories (preserve original behavior)
        if locations.len() > 1 && i < locations.len() - 1 {
            println!();
        }
    }
    
    // Write collected diffs if --output-diff specified
    if let Some(ref diff_path) = args.output_diff {
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
