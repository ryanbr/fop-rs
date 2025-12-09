//! FOP - Filter Orderer and Preener
//! 
//! A tool for sorting and cleaning ad-blocking filter lists.
//! Rust port of the original Python FOP by Michael (EasyList project).
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

use std::collections::HashMap;
use ahash::AHashSet as HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::collections::HashMap as StdHashMap;
use std::process::Command;

// ANSI color codes
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";

use once_cell::sync::Lazy;
use regex::Regex;
use walkdir::WalkDir;
use rayon::prelude::*;

// FOP version number
const VERSION: &str = "3.9-rs";

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
    /// Show help
    help: bool,
    /// Show version
    version: bool,
}

/// Load configuration from .fopconfig file
fn load_config(custom_path: Option<&PathBuf>) -> StdHashMap<String, String> {
    let mut config = StdHashMap::new();
    
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
    
    config
}

/// Parse boolean value from config
fn parse_bool(config: &StdHashMap<String, String>, key: &str, default: bool) -> bool {
    config.get(key).map(|v| {
        matches!(v.to_lowercase().as_str(), "true" | "yes" | "1")
    }).unwrap_or(default)
}

/// Parse string list from config (comma-separated)
fn parse_list(config: &StdHashMap<String, String>, key: &str) -> Vec<String> {
    config.get(key).map(|v| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }).unwrap_or_default()
}

impl Args {
    fn parse() -> Self {
        // First pass: look for --config-file argument
        let mut config_file: Option<PathBuf> = None;
        for arg in env::args().skip(1) {
            if arg.starts_with("--config-file=") {
                let path = arg.trim_start_matches("--config-file=");
                config_file = Some(PathBuf::from(path));
                break;
            }
        }
        
        // Load config file
        let config = load_config(config_file.as_ref());
        
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
                _ if arg.starts_with("--ignorefiles=") => {
                    let files = arg.trim_start_matches("--ignorefiles=");
                    args.ignore_files = files.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ if arg.starts_with("--config-file=") => {
                    // Already handled in first pass
                }
                _ if arg.starts_with('-') => {
                    eprintln!("Unknown option: {}", arg);
                    eprintln!("Use --help for usage information");
                    std::process::exit(1);
                }
                _ => args.directories.push(PathBuf::from(arg)),
            }
        }

        args
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
        println!("        --ignorefiles=  Additional files to ignore (comma-separated, partial names)");
        println!("        --config-file=  Custom config file path");
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
        println!();
        println!("Config file (.fopconfig):");
        println!("    Place in current directory or home directory.");
        println!("    Command line arguments override config file settings.");
        println!();
        println!("    # Example .fopconfig");
        println!("    no-commit = true");
        println!("    no-ubo-convert = false");
        println!("    ignorefiles = .json,.backup,test.txt");
    }

    fn print_version() {
        println!("FOP {}", VERSION);
    }
}

// =============================================================================
// Regular Expression Patterns
// =============================================================================

static ELEMENT_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)#@?#"#).unwrap()
});

static FILTER_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:\$|,)domain=([^,\s]+)$").unwrap()
});

/// Pattern for FOP.py compatible element matching (no {} in selector)
static FOPPY_ELEMENT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)(#[@?]?#)([^{}]+)$"#).unwrap()
});

/// Pattern for FOP.py compatible sorting (only ## and #@#)
static FOPPY_ELEMENT_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^[^/|@"!]*?#@?#"#).unwrap()
});

/// Pattern for element hiding rules (standard, uBO, and AdGuard extended syntax)
static ELEMENT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^/|@"!]*?)(##|#@#|#\?#|#@\?#|#\$#|#@\$#|#%#|#@%#)(.+)$"#).unwrap()
});

/// Pattern for regex domain element hiding rules (uBO/AdGuard specific)
static REGEX_ELEMENT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^(/[^#]+/)(##|#@#|#\?#|#@\?#|#\$#|#@\$#|#%#|#@%#)(.+)$"#).unwrap()
});

/// Pattern for localhost/hosts file entries
static LOCALHOST_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(0\.0\.0\.0|127\.0\.0\.1)\s+(.+)$").unwrap()
});

static OPTION_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(.*)\$(~?[\w\-]+(?:=[^,\s]+)?(?:,~?[\w\-]+(?:=[^,\s]+)?)*)$").unwrap()
});

static PSEUDO_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(:[a-zA-Z\-]*[A-Z][a-zA-Z\-]*)").unwrap()
});

static REMOVAL_PATTERN: Lazy<Regex> = Lazy::new(|| {
    // Simplified pattern - matches asterisks that can be removed
    // Original used lookahead: r"([>+~,@\s])(\*)([#.\[]|:(?!-abp-contains))"
    // We'll handle the -abp-contains check in code
    Regex::new(r"([>+~,@\s])(\*)([#.\[:])")
        .expect("Invalid REMOVAL_PATTERN regex")
});

static ATTRIBUTE_VALUE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^([^'"\\]|\\.)*("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|\*"#).unwrap()
});

static TREE_SELECTOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\\.|[^+>~ \t])\s*([+>~ \t])\s*(\D)").unwrap()
});

static UNICODE_SELECTOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\\[0-9a-fA-F]{1,6}\s[a-zA-Z]*[A-Z]").unwrap()
});

static TLD_ONLY_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\|\||[|])?\.([a-z]{2,})\^?$").unwrap()
});

static COMMIT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(A|M|P):\s(\((.+)\)\s)?(.*)$").unwrap()
});

static SHORT_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\|*[a-zA-Z0-9]").unwrap()
});

static DOMAIN_EXTRACT_PATTERN: Lazy<Regex> = Lazy::new(|| {
    // Don't include \* - wildcards are valid in domain patterns like ||stats-*.example.com^
    Regex::new(r"^\|*([^/\^\$]+)").unwrap()
});

static IP_ADDRESS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\d+\.\d+\.\d+\.\d+").unwrap()
});

// =============================================================================
// Constants
// =============================================================================

/// Files that should not be sorted
const IGNORE_FILES: &[&str] = &[
    "CC-BY-SA.txt", "easytest.txt", "GPL.txt", "MPL.txt",
    "easylist_specific_hide_abp.txt", "easyprivacy_specific_uBO.txt",
    "enhancedstats-addon.txt", "fanboy-tracking", "firefox-regional", "other",
    "easylist_cookie_specific_uBO.txt", "fanboy_annoyance_specific_uBO.txt",
    "fanboy_newsletter_specific_uBO.txt", "fanboy_notifications_specific_uBO.txt",
    "fanboy_social_specific_uBO.txt", "fanboy_newsletter_shopping_specific_uBO.txt",
    "fanboy_agegate_specific_uBO.txt", "config-clean2.json", "config-clean.json",
    "config-clean.json.txt", "config-clean2.json.txt", "config-clean2.txt",
    "config-clean.txt",
];

/// Directories to ignore
const IGNORE_DIRS: &[&str] = &[
    "fanboy-tracking", "firefox-regional", "other",
];

/// Domains that should ignore the 7 character size restriction
static IGNORE_DOMAINS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    set.insert("a.sampl");
    set
});

/// Known Adblock Plus options (HashSet for O(1) lookup)
static KNOWN_OPTIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
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
// UBO Option Conversion
// =============================================================================

/// uBO to ABP option conversions
static UBO_CONVERSIONS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
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

/// Convert uBO-specific options to standard ABP options
fn convert_ubo_options(options: Vec<String>) -> Vec<String> {

    options.into_iter().map(|option| {
        if option.starts_with("from=") {
            option.replacen("from=", "domain=", 1)
        } else {
            UBO_CONVERSIONS.get(option.as_str())
                .map(|s| s.to_string())
                .unwrap_or(option)
        }
    }).collect()
}

/// Sort domains alphabetically, ignoring ~ prefix
fn sort_domains(domains: &mut Vec<String>) {
    domains.sort_by(|a, b| a.trim_start_matches('~').cmp(b.trim_start_matches('~')));
}

// =============================================================================
// Filter Processing Functions
// =============================================================================

/// Remove unnecessary wildcards from filter text
fn remove_unnecessary_wildcards(filter_text: &str) -> String {
    let mut result = filter_text.to_string();
    let allowlist = result.starts_with("@@");

    if allowlist {
        result = result[2..].to_string();
    }
    
    let original_len = result.len();

    // Remove leading asterisks
    while result.len() > 1 && result.starts_with('*')
        && !result[1..].starts_with('|') && !result[1..].starts_with('!') {
        result.remove(0);
    }

    // Remove trailing asterisks
    while result.len() > 1 && result.ends_with('*')
        && !result[..result.len()-1].ends_with('|') && !result[..result.len()-1].ends_with(' ') {
        result.pop();
    }

    // Handle regex patterns
    let had_star = result.len() != original_len;
    if had_star && result.starts_with('/') && result.ends_with('/') {
        result.push('*');
    }

    if result == "*" {
        result.clear();
    }

    if allowlist {
        result.insert_str(0, "@@");
    }

    result
}

/// Sort and clean filter options
fn filter_tidy(filter_in: &str, convert_ubo: bool) -> String {
    let option_split = OPTION_PATTERN.captures(filter_in);

    match option_split {
        None => remove_unnecessary_wildcards(filter_in),
        Some(caps) => {
            let filter_text = remove_unnecessary_wildcards(&caps[1]);
            let option_list: Vec<String> = caps[2].split(',').map(|opt| {
                // Only replace underscores in option name, not in value
                if let Some(eq_pos) = opt.find('=') {
                    let name = opt[..eq_pos].to_lowercase().replace('_', "-");
                    let value = &opt[eq_pos..]; // Keep value as-is (preserve case and underscores)
                    format!("{}{}", name, value)
                } else {
                    opt.to_lowercase().replace('_', "-")
                }
            }).collect();
            
            // Convert uBO options
            let option_list = if convert_ubo {
                convert_ubo_options(option_list)
            } else {
                option_list
            };

            let mut domain_list: Vec<String> = Vec::new();
            let mut remove_entries: HashSet<String> = HashSet::new();
            let mut final_options: Vec<String> = Vec::new();

            for option in &option_list {
                if option.starts_with("domain=") {
                    let domains = &option[7..];
                    domain_list.extend(domains.split('|').map(String::from));
                    remove_entries.insert(option.clone());
                } else {
                    let stripped = option.trim_start_matches('~');
                    // Check if option is known (exact match or known prefix)
                    let is_known = KNOWN_OPTIONS.contains(stripped)
                        || stripped.starts_with("csp=")
                        || stripped.starts_with("redirect=")
                        || stripped.starts_with("redirect-rule=")
                        || stripped.starts_with("rewrite=")
                        || stripped.starts_with("replace=")
                        || stripped.starts_with("header=")
                        || stripped.starts_with("permissions=")
                        || stripped.starts_with("to=")
                        || stripped.starts_with("from=")
                        || stripped.starts_with("method=")
                        || stripped.starts_with("denyallow=")
                        || stripped.starts_with("removeparam=")
                        || stripped.starts_with("urltransform=")
                        || stripped.starts_with("responseheader=")
                        || stripped.starts_with("urlskip=");
                    if !is_known {
                        eprintln!(
                            "Warning: The option \"{}\" used on the filter \"{}\" is not recognised by FOP",
                            option, filter_in
                        );
                    }
                }
            }

            // Sort options alphabetically, with inverse following non-inverse
            let mut sorted_options: Vec<String> = option_list
                .into_iter()
                .filter(|opt| !remove_entries.contains(opt))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            sorted_options.sort_by(|a, b| {
                let key_a = if a.starts_with('~') {
                    format!("{}~", &a[1..])
                } else {
                    a.clone()
                };
                let key_b = if b.starts_with('~') {
                    format!("{}~", &b[1..])
                } else {
                    b.clone()
                };
                key_a.cmp(&key_b)
            });

            final_options.extend(sorted_options);

            // Sort and append domain restrictions
            if !domain_list.is_empty() {
                let mut unique_domains: Vec<String> = domain_list
                    .into_iter()
                    .filter(|d| !d.is_empty())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();

                sort_domains(&mut unique_domains);

                final_options.push(format!("domain={}", unique_domains.join("|")));
            }

            format!("{}${}", filter_text, final_options.join(","))
        }
    }
}

/// Sort domains and clean element hiding rules
fn element_tidy(domains: &str, separator: &str, selector: &str) -> String {
    let mut domains = domains.to_lowercase();

    // Sort domain names alphabetically
    if domains.contains(',') {
        let domain_list: Vec<&str> = domains.split(',').collect();
        let mut valid_domains: Vec<String> = Vec::new();
        let mut invalid_domains: Vec<String> = Vec::new();

        for d in domain_list {
            let stripped = d.trim_start_matches('~');
            if stripped.len() < 3 || !stripped.contains('.') {
                invalid_domains.push(d.to_string());
            } else {
                valid_domains.push(d.to_string());
            }
        }

        if !invalid_domains.is_empty() {
            eprintln!(
                "Removed invalid domain(s) from cosmetic rule: {}",
                invalid_domains.join(", ")
            );
        }

        sort_domains(&mut valid_domains);
        valid_domains.dedup();
        domains = valid_domains.join(",");
    }

    // Skip selector processing for uBO/ABP/AdGuard extended syntax (preserve exactly as-is)
    let is_extended = selector.starts_with("+js(")
        || selector.starts_with("^")
        || selector.starts_with("//scriptlet(")
        || selector.contains(":style(")
        || selector.contains(":has-text(")
        || selector.contains(":has(")
        || selector.contains(":remove(")
        || selector.contains(":remove-attr(")
        || selector.contains(":remove-class(")
        || selector.contains(":matches-path(")
        || selector.contains(":matches-css(")
        || selector.contains(":matches-media(")
        || selector.contains(":matches-prop(")
        || selector.contains(":upward(")
        || selector.contains(":xpath(")
        || selector.contains(":watch-attr(")
        || selector.contains(":min-text-length(")
        || selector.contains(":-abp-has(")
        || selector.contains(":-abp-contains(")
        || selector.contains(":-abp-properties(")
        || selector.contains(":others(")
        || selector.contains(" {")
        || separator == "#$#"
        || separator == "#@$#"
        || separator == "#%#"
        || separator == "#@%#";

    if is_extended {
        return format!("{}{}{}", domains, separator, selector);
    }

    // Mark selector boundaries
    let mut selector = format!("@{}@", selector);

    // Extract strings to avoid modifying content inside them
    let mut selector_without_strings = selector.clone();
    let mut selector_only_strings = String::new();

    loop {
        let caps = ATTRIBUTE_VALUE_PATTERN.captures(&selector_without_strings);
        match caps {
            Some(c) => {
                if let Some(string_part) = c.get(2) {
                    let before = c.get(1).map(|m| m.as_str()).unwrap_or("");
                    let string_val = string_part.as_str().to_string();
                    let full_match = format!("{}{}", before, string_val);
                    selector_without_strings = selector_without_strings.replacen(&full_match, before, 1);
                    selector_only_strings.push_str(&string_val);
                } else {
                    break;
                }
            }
            None => break,
        }
    }

    // Clean up tree selectors
    for caps in TREE_SELECTOR.captures_iter(&selector.clone()) {
        let full_match = caps.get(0).unwrap().as_str();
        if selector_only_strings.contains(full_match) || !selector_without_strings.contains(full_match) {
            continue;
        }

        let g1 = &caps[1];
        let g2 = &caps[2];
        let g3 = &caps[3];

        // Skip if g1 is a backslash - this means g2 is part of an escape sequence (\~, \+, etc.)
        // Not a CSS combinator
        if g1 == "\\" {
            continue;
        }

        // Skip if g1 is an escaped quote (we're at a string boundary)
        // This prevents mangling content like url('~/path') where ~ is not a combinator
        if g1 == "\\'" || g1 == "\\\"" {
            continue;
        }

        let replace_by = if g1 == "(" {
            format!("{} ", g2)
        } else {
            format!(" {} ", g2)
        };

        let replace_by = if replace_by == "   " { " ".to_string() } else { replace_by };
        selector = selector.replacen(full_match, &format!("{}{}{}", g1, replace_by, g3), 1);
    }

    // Remove unnecessary tags (asterisks)
    for caps in REMOVAL_PATTERN.captures_iter(&selector.clone()) {
        if let Some(untag) = caps.get(2) {
            let untag_name = untag.as_str();
            if selector_only_strings.contains(untag_name) || !selector_without_strings.contains(untag_name) {
                continue;
            }

            let bc = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let ac = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            
            // Skip if this is a :not(-abp-contains...) pattern
            if ac == ":" {
                let match_end = caps.get(0).unwrap().end();
                if selector[match_end..].starts_with("-abp-contains") {
                    continue;
                }
            }
            
            let old = format!("{}{}{}", bc, untag_name, ac);
            let new = format!("{}{}", bc, ac);
            selector = selector.replacen(&old, &new, 1);
        }
    }

    // Make pseudo classes lowercase
    for caps in PSEUDO_PATTERN.captures_iter(&selector.clone()) {
        let pseudo_class = &caps[1];
        if selector_only_strings.contains(pseudo_class) || !selector_without_strings.contains(pseudo_class) {
            continue;
        }

        if UNICODE_SELECTOR.is_match(&selector_without_strings) {
            break;
        }

        let ac = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let old = format!("{}{}", pseudo_class, ac);
        let new = format!("{}{}", pseudo_class.to_lowercase(), ac);
        selector = selector.replacen(&old, &new, 1);
    }

    // Remove markers and return complete rule
    let selector = &selector[1..selector.len()-1];
    format!("{}{}{}", domains, separator, selector)
}

/// Combine filters with identical rules but different domains
fn combine_filters(
    mut uncombined: Vec<String>,
    domain_pattern: &Regex,
    separator: &str,
) -> Vec<String> {
    let mut combined: Vec<String> = Vec::new();

    for i in 0..uncombined.len() {
        let domains1 = domain_pattern.captures(&uncombined[i]);
        
        // Get domain info for current and next filter
        let (domain1_str, domains1_full) = if i + 1 < uncombined.len() {
            if let Some(ref caps) = domains1 {
                (
                    caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                    caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default(),
                )
            } else {
                (String::new(), String::new())
            }
        } else {
            (String::new(), String::new())
        };

        let domains2 = if i + 1 < uncombined.len() {
            domain_pattern.captures(&uncombined[i + 1])
        } else {
            None
        };

        // Check if we should just add current filter without combining
        if domains1.is_none() 
            || i + 1 >= uncombined.len() 
            || domains2.is_none() 
            || domain1_str.is_empty()
        {
            combined.push(std::mem::take(&mut uncombined[i]));
            continue;
        }

        let domain2_str = domains2.as_ref()
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");

        if domain2_str.is_empty() {
            combined.push(std::mem::take(&mut uncombined[i]));
            continue;
        }

        let domains2_full = domains2.as_ref()
            .and_then(|c| c.get(0))
            .map(|m| m.as_str())
            .unwrap_or("");

        // Check if domain patterns are compatible (same structure except domain list)
        let pattern1_with_domain2 = domains1_full.replace(&domain1_str, domain2_str);
        if pattern1_with_domain2 != domains2_full {
            combined.push(std::mem::take(&mut uncombined[i]));
            continue;
        }

        // Check if filters are identical except for domains
        let filter1_no_domain = domain_pattern.replace(&uncombined[i], "");
        let filter2_no_domain = domain_pattern.replace(&uncombined[i + 1], "");

        if filter1_no_domain != filter2_no_domain {
            combined.push(std::mem::take(&mut uncombined[i]));
            continue;
        }

        // Check for mixed include/exclude domains
        let domain1_exclude_count = domain1_str.matches('~').count();
        let domain1_total = domain1_str.split(separator).count();
        let domain2_exclude_count = domain2_str.matches('~').count();
        let domain2_total = domain2_str.split(separator).count();

        let domain1_only_excludes = domain1_exclude_count == domain1_total;
        let domain2_only_excludes = domain2_exclude_count == domain2_total;

        if domain1_only_excludes != domain2_only_excludes {
            combined.push(std::mem::take(&mut uncombined[i]));
            continue;
        }

        // Combine domains
        let mut new_domains: Vec<String> = domain1_str
            .split(separator)
            .chain(domain2_str.split(separator))
            .map(String::from)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        new_domains.sort_by(|a, b| {
            a.trim_start_matches('~').cmp(b.trim_start_matches('~'))
        });

        let new_domain_str = new_domains.join(separator);
        
        // Create the substitution pattern (full match with new domains)
        let domains_substitute = domains1_full.replace(&domain1_str, &new_domain_str);
        
        // Escape $ for regex replacement ($ is special in replacement strings)
        let escaped_substitute = domains_substitute.replace("$", "$$");
        
        // Modify the next filter to be the combined version
        // (using filter i as the base, replacing its domain pattern with the combined domains)
        let combined_filter = domain_pattern.replace(&uncombined[i], escaped_substitute.as_str()).to_string();
        uncombined[i + 1] = combined_filter;
        
        // Don't add current filter to combined - it will be processed as part of next iteration
    }

    combined
}

// =============================================================================
// File Processing
// =============================================================================

/// Sort the sections of a filter file and save modifications
fn fop_sort(filename: &Path, convert_ubo: bool, no_sort: bool, alt_sort: bool, localhost: bool) -> io::Result<()> {
    let temp_file = filename.with_extension("temp");
    const CHECK_LINES: usize = 10;

    let input = File::open(filename)?;
    let reader = BufReader::new(input);
    let mut output = BufWriter::with_capacity(64 * 1024, File::create(&temp_file)?);

    let mut section: Vec<String> = Vec::with_capacity(800);
    let mut lines_checked: usize = 1;
    let mut filter_lines: usize = 0;
    let mut element_lines: usize = 0;

    let write_filters = |section: &mut Vec<String>, 
                         output: &mut BufWriter<File>,  
                         element_lines: usize, 
                         filter_lines: usize,
                         no_sort: bool,
                         alt_sort: bool,
                         localhost: bool| -> io::Result<()> {
        if section.is_empty() {
            return Ok(());
        }

        // Remove duplicates while preserving order if no_sort
        let mut unique: Vec<String> = if no_sort {
            let mut seen = HashSet::new();
            section.drain(..).filter(|x| seen.insert(x.clone())).collect()
        } else {
            section.drain(..).collect::<HashSet<_>>().into_iter().collect()
        };

        if localhost {
            // Sort hosts file entries by domain
            if !no_sort {
                unique.sort_by(|a, b| {
                    let a_domain = LOCALHOST_PATTERN.captures(a).map(|c| c[2].to_lowercase()).unwrap_or_else(|| a.to_lowercase());
                    let b_domain = LOCALHOST_PATTERN.captures(b).map(|c| c[2].to_lowercase()).unwrap_or_else(|| b.to_lowercase());
                    a_domain.cmp(&b_domain)
                });
            }
            for filter in unique {
                writeln!(output, "{}", filter)?;
            }
        } else if element_lines > filter_lines {
            if !no_sort {
                let pattern = if alt_sort {
                    &*ELEMENT_DOMAIN_PATTERN
                } else {
                    &*FOPPY_ELEMENT_DOMAIN_PATTERN
                };
                unique.sort_by(|a, b| {
                    let a_key = pattern.replace(a, "");
                    let b_key = pattern.replace(b, "");
                    a_key.cmp(&b_key)
                });
            }
            let combined = combine_filters(unique, &ELEMENT_DOMAIN_PATTERN, ",");
            for filter in combined {
                writeln!(output, "{}", filter)?;
            }
        } else {
            // Sort blocking rules (unless no_sort)
            if !no_sort {
                unique.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            }
            let combined = combine_filters(unique, &FILTER_DOMAIN_PATTERN, "|");
            for filter in combined {
                writeln!(output, "{}", filter)?;
            }
        }

        Ok(())
    };

    for line in reader.lines() {
        let line = line?.trim().to_string();

        if line.is_empty() {
            continue;
        }

        // Comments and special lines
        let is_comment = line.starts_with('!')
            || (localhost && line.starts_with('#'));
        if is_comment
            || line.starts_with("%include")
            || (line.starts_with('[') && line.ends_with(']'))
        {
            if !section.is_empty() {
                write_filters(&mut section, &mut output, element_lines, filter_lines, no_sort, alt_sort, localhost)?;
                lines_checked = 1;
                filter_lines = 0;
                element_lines = 0;
            }
            writeln!(output, "{}", line)?;
            continue;
        }

        // Skip filters less than 3 characters
        if line.len() < 3 {
            continue;
        }

        // Handle regex domain rules (uBO) - pass through unchanged
        if REGEX_ELEMENT_PATTERN.is_match(&line) {
            section.push(line.clone());
            continue;
        }

        // Process element hiding rules
        let element_caps = if alt_sort {
            ELEMENT_PATTERN.captures(&line)
        } else {
            FOPPY_ELEMENT_PATTERN.captures(&line)
        };
        if let Some(caps) = element_caps {
            let domains = caps[1].to_lowercase();
            let separator = &caps[2];
            let selector = &caps[3];

            if lines_checked <= CHECK_LINES {
                element_lines += 1;
                lines_checked += 1;
            }

            let tidied = element_tidy(&domains, separator, selector);
            section.push(tidied);
            continue;
        }

        // Process blocking rules
               
        // Skip short domain rules
        if line.len() <= 7 && SHORT_DOMAIN_PATTERN.is_match(&line) {
            if let Some(caps) = DOMAIN_EXTRACT_PATTERN.captures(&line) {
                let domain = &caps[1];
                if !IGNORE_DOMAINS.contains(domain) {
                    eprintln!("Skipped short domain rule: {} (domain: {})", line, domain);
                    continue;
                }
            }
        }

        // Skip network rules without dot in domain
        let skip_schemes = ["|javascript", "|data:", "|dddata:", "|about:", "|blob:", "|http", "||edge-client"];
        if (line.starts_with("||") || line.starts_with('|'))
            && !skip_schemes.iter().any(|s| line.starts_with(s))
        {
            if let Some(caps) = DOMAIN_EXTRACT_PATTERN.captures(&line) {
                let domain = &caps[1];
                let is_ip = domain.starts_with('[') || IP_ADDRESS_PATTERN.is_match(domain);
                let has_wildcard = domain.contains('*');

                if !is_ip && !has_wildcard && !domain.contains('.') && !domain.starts_with('~') {
                    eprintln!("Skipped network rule without dot in domain: {} (domain: {})", line, domain);
                    continue;
                }
            }
        }

        // Remove TLD-only patterns
        if TLD_ONLY_PATTERN.is_match(&line) {
            eprintln!("Removed overly broad TLD-only rule: {}", line);
            continue;
        }

        if lines_checked <= CHECK_LINES {
            filter_lines += 1;
            lines_checked += 1;
        }

        let tidied = filter_tidy(&line, convert_ubo);
        section.push(tidied);
    }

    // Write remaining filters
    if !section.is_empty() {
        write_filters(&mut section, &mut output, element_lines, filter_lines, no_sort, alt_sort, localhost)?;
    }

    drop(output);

    // Compare files and replace if different
    let original_content = fs::read(filename)?;
    let new_content = fs::read(&temp_file)?;

    if original_content != new_content {
        fs::rename(&temp_file, filename)?;
        println!("Sorted: {}", filename.display());
    } else {
        fs::remove_file(&temp_file)?;
    }

    Ok(())
}

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

fn valid_url(url_str: &str) -> bool {
    // Handle about: URLs specially
    if url_str.starts_with("about:") {
        return true;
    }

    // Simple URL validation: check for scheme://host/path pattern
    // Look for "://" which indicates scheme separator
    if let Some(scheme_end) = url_str.find("://") {
        let scheme = &url_str[..scheme_end];
        // Scheme should be alphanumeric (like http, https, ftp)
        if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }

        let rest = &url_str[scheme_end + 3..];
        // There should be something after ://
        if rest.is_empty() {
            return false;
        }

        // Check for a host (anything before the first /)
        let host_end = rest.find('/').unwrap_or(rest.len());
        let host = &rest[..host_end];

        // Host should not be empty (or be at least a character)
        if host.is_empty() {
            return false;
        }

        // Check for path (after host, there should be at least / or nothing)
        // If there's something after host, it starts with / which is the path
        // Since we accept both "http://example.com" and "http://example.com/"
        // we consider empty path as valid (implicit "/")
        return true;
    }

    false
}

fn check_comment(comment: &str, user_changes: bool) -> bool {
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
        println!("{}{}{}", GREEN, line, RESET);
    } else if line.starts_with('-') && !line.starts_with("---") {
        println!("{}{}{}", RED, line, RESET);
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

    // Check for large changes
    if !original_difference && is_large_change(&diff) {
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

fn process_location(location: &Path, no_commit: bool, convert_ubo: bool, no_msg_check: bool, disable_ignored: bool, no_sort: bool, alt_sort: bool, localhost: bool, no_color: bool, ignore_files: &[String]) -> io::Result<()> {
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
            !name.starts_with('.') && (disable_ignored || !IGNORE_DIRS.contains(&name.as_ref()))
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
            commit_changes(repo, &base_cmd, original_difference, no_msg_check, no_color)?;
        }
    }

    Ok(())
}

fn print_greeting(no_commit: bool) {
    let mode = if no_commit { " (sort only)" } else { "" };
    let greeting = format!("FOP (Filter Orderer and Preener) version {}{}", VERSION, mode);
    let separator = "=".repeat(greeting.len());
    println!("{}", separator);
    println!("{}", greeting);
    println!("{}", separator);
}

fn main() {
    let args = Args::parse();

    // Handle help and version
    if args.help {
        Args::print_help();
        return;
    }

    if args.version {
        Args::print_version();
        return;
    }

    print_greeting(args.no_commit);

    if args.directories.is_empty() {
        // Process current directory
        if let Ok(cwd) = env::current_dir() {
            if let Err(e) = process_location(&cwd, args.no_commit, !args.no_ubo_convert, args.no_msg_check, args.disable_ignored, args.no_sort, args.alt_sort, args.localhost, args.no_color, &args.ignore_files) {
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
            if let Err(e) = process_location(&place, args.no_commit, !args.no_ubo_convert, args.no_msg_check, args.disable_ignored, args.no_sort, args.alt_sort, args.localhost, args.no_color, &args.ignore_files) {
                eprintln!("Error: {}", e);
            }
            println!();
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_unnecessary_wildcards() {
        assert_eq!(remove_unnecessary_wildcards("*example*"), "example");
        assert_eq!(remove_unnecessary_wildcards("**example**"), "example");
        assert_eq!(remove_unnecessary_wildcards("@@*example*"), "@@example");
        assert_eq!(remove_unnecessary_wildcards("*|example"), "*|example");
        assert_eq!(remove_unnecessary_wildcards("example|*"), "example|*");
    }

    #[test]
    fn test_convert_ubo_options() {
        let input = vec!["xhr".to_string(), "3p".to_string(), "frame".to_string()];
        let expected = vec!["xmlhttprequest", "third-party", "subdocument"];
        let result = convert_ubo_options(input);
        assert_eq!(result, expected);

        let input2 = vec!["from=example.com".to_string()];
        let result2 = convert_ubo_options(input2);
        assert_eq!(result2, vec!["domain=example.com"]);
    }

    #[test]
    fn test_filter_tidy() {
        // Test option sorting
        let result = filter_tidy("||example.com^$image,script,third-party", true);
        assert!(result.contains("image"));
        assert!(result.contains("script"));
        assert!(result.contains("third-party"));

        // Test domain sorting
        let result = filter_tidy("||ad.com^$domain=z.com|a.com|m.com", true);
        assert!(result.contains("domain=a.com|m.com|z.com"));
    }

    #[test]
    fn test_element_tidy() {
        let result = element_tidy("z.com,a.com,m.com", "##", ".ad");
        assert!(result.starts_with("a.com,m.com,z.com"));
        
        // Test uBO scriptlet - selector preserved exactly
        let result = element_tidy("z.com,a.com", "##", "+js(nowoif)");
        assert_eq!(result, "a.com,z.com##+js(nowoif)");
        
        // Test uBO :has-text() - selector preserved
        let result = element_tidy("example.com", "##", "div:has-text(Sponsored)");
        assert_eq!(result, "example.com##div:has-text(Sponsored)");
        
        // Test uBO :style() - selector preserved
        let result = element_tidy("site.com", "##", "body:style(overflow: auto !important)");
        assert_eq!(result, "site.com##body:style(overflow: auto !important)");
        
        // Test AdGuard scriptlet
        let result = element_tidy("example.com", "#%#", "//scriptlet('set-cookie', 'a', 'b')");
        assert_eq!(result, "example.com#%#//scriptlet('set-cookie', 'a', 'b')");
        
        // Test ABP extended selector
        let result = element_tidy("site.com", "#?#", "div:-abp-has(span:-abp-contains(Ad))");
        assert_eq!(result, "site.com#?#div:-abp-has(span:-abp-contains(Ad))");
        
        // Test ABP action syntax
        let result = element_tidy("site.com", "##", ".ad {remove: true;}");
        assert_eq!(result, "site.com##.ad {remove: true;}");
        
        // Test HTML filtering
        let result = element_tidy("site.com", "##", "^script:has-text(ads)");
        assert_eq!(result, "site.com##^script:has-text(ads)");
    }

    #[test]
    fn test_filter_tidy_ubo_options() {
        // Test redirect= is recognized
        let result = filter_tidy("||ads.com^$script,redirect=noopjs", true);
        assert!(result.contains("redirect=noopjs"));
        
        // Test denyallow= is recognized
        let result = filter_tidy("*$script,3p,denyallow=cdn.com", true);
        assert!(result.contains("denyallow=cdn.com"));
        
        // Test removeparam= is recognized
        let result = filter_tidy("||site.com^$removeparam=utm_source", true);
        assert!(result.contains("removeparam=utm_source"));
        
        // Test no-ubo-convert mode
        let result = filter_tidy("||ads.com^$xhr,3p", false);
        assert!(result.contains("xhr"));
        assert!(result.contains("3p"));
        
        // Test ubo-convert mode (default)
        let result = filter_tidy("||ads.com^$xhr,3p", true);
        assert!(result.contains("xmlhttprequest"));
        assert!(result.contains("third-party"));
    }

    #[test]
    fn test_regex_element_pattern() {
        // Regex domain rules should match
        assert!(REGEX_ELEMENT_PATTERN.is_match("/tamilprint\\d+/##+js(nowoif)"));
        assert!(REGEX_ELEMENT_PATTERN.is_match("/regex/##.selector"));
        
        // Normal rules should not match
        assert!(!REGEX_ELEMENT_PATTERN.is_match("example.com##.ad"));
    }

    #[test]
    fn test_valid_url() {
        assert!(valid_url("https://example.com/path"));
        assert!(valid_url("http://test.org/"));
        assert!(valid_url("about:blank"));
        assert!(!valid_url("not-a-url"));
        assert!(!valid_url("example.com")); // Missing scheme
    }

    #[test]
    fn test_tld_only_pattern() {
        assert!(TLD_ONLY_PATTERN.is_match("||.org^"));
        assert!(TLD_ONLY_PATTERN.is_match(".com"));
        assert!(TLD_ONLY_PATTERN.is_match("|.net^"));
        assert!(!TLD_ONLY_PATTERN.is_match("||example.org^"));
    }

    #[test]
    fn test_check_comment() {
        assert!(check_comment("M: Fixed typo", false));
        assert!(check_comment("A: (filters) https://example.com/issue", true));
        assert!(!check_comment("Invalid comment", false));
        assert!(!check_comment("A: (filters) not-a-url", true));
    }

    #[test]
    fn test_localhost_pattern() {
        // Test 0.0.0.0 entries
        assert!(LOCALHOST_PATTERN.is_match("0.0.0.0 domain.com"));
        assert!(LOCALHOST_PATTERN.is_match("0.0.0.0 sub.domain.com"));
        assert!(LOCALHOST_PATTERN.is_match("0.0.0.0 ads.example.org"));
        
        // Test 127.0.0.1 entries
        assert!(LOCALHOST_PATTERN.is_match("127.0.0.1 domain.com"));
        assert!(LOCALHOST_PATTERN.is_match("127.0.0.1 sub.domain.com"));
        assert!(LOCALHOST_PATTERN.is_match("127.0.0.1 tracker.net"));
        
        // Test non-matching entries
        assert!(!LOCALHOST_PATTERN.is_match("# comment"));
        assert!(!LOCALHOST_PATTERN.is_match("192.168.1.1 domain.com"));
        assert!(!LOCALHOST_PATTERN.is_match("domain.com"));
    }

    #[test]
    fn test_localhost_domain_extraction() {
        let caps = LOCALHOST_PATTERN.captures("0.0.0.0 z-ads.com").unwrap();
        assert_eq!(&caps[2], "z-ads.com");
        
        let caps = LOCALHOST_PATTERN.captures("127.0.0.1 sub.domain.com").unwrap();
        assert_eq!(&caps[2], "sub.domain.com");
        
        let caps = LOCALHOST_PATTERN.captures("0.0.0.0 a-tracker.net").unwrap();
        assert_eq!(&caps[2], "a-tracker.net");
    }
}
