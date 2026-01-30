//! Filter sorting and tidying logic
//!
//! Copyright (C) 2025 FanboyNZ (FOP Rust)
//! https://github.com/ryanbr/fop-rs
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::io::Cursor;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use owo_colors::OwoColorize;
use ahash::AHashSet as HashSet;
use ahash::AHashMap;
use regex::Regex;
use std::cmp::Ordering;

use crate::{
    write_warning, ATTRIBUTE_VALUE_PATTERN, DOMAIN_EXTRACT_PATTERN, ELEMENT_DOMAIN_PATTERN,
    ELEMENT_PATTERN, FILTER_DOMAIN_PATTERN, FOPPY_ELEMENT_DOMAIN_PATTERN, FOPPY_ELEMENT_PATTERN,
    IP_ADDRESS_PATTERN, KNOWN_OPTIONS, LOCALHOST_PATTERN, OPTION_PATTERN,
    PSEUDO_PATTERN, REGEX_ELEMENT_PATTERN, REMOVAL_PATTERN, TREE_SELECTOR,
    UBO_CONVERSIONS, UNICODE_SELECTOR,
};

use crate::fop_typos;

// Pattern for :has-text() merging
use std::sync::LazyLock;
static HAS_TEXT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Match :has-text() at end, not followed by other pseudo-classes
    Regex::new(r"^(.+?):(has-text|-?abp-contains)\((.+)\)$").unwrap()
});

// Skip network-rule scheme/edge prefixes that don't require a dot in the "domain" part.
const SKIP_SCHEMES: [&str; 6] = [
    "|javascript", "|data:", "|dddata:", "|about:", "|blob:", "|http",
];

/// Case-insensitive ASCII comparison without allocation
#[inline]
fn cmp_ascii_case_insensitive(a: &str, b: &str) -> Ordering {
    let mut ai = a.bytes();
    let mut bi = b.bytes();
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) => {
                let al = ac.to_ascii_lowercase();
                let bl = bc.to_ascii_lowercase();
                if al != bl {
                    return al.cmp(&bl);
                }
            }
        }
    }
}

/// Check if line is a TLD-only pattern (e.g. .com, ||.net^)
/// Replaces regex: r"^(\|\||[|])?\.([a-z]{2,})\^?$"
#[inline]
pub fn is_tld_only(line: &str) -> bool {
    let s = if let Some(rest) = line.strip_prefix("||") {
        rest
    } else if let Some(rest) = line.strip_prefix('|') {
        rest
    } else {
        line
    };
    let s = s.strip_prefix('.').unwrap_or("");
    let s = s.strip_suffix('^').unwrap_or(s);
    s.len() >= 2 && s.bytes().all(|b| b.is_ascii_lowercase())
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for sorting operations
pub struct SortConfig<'a> {
    pub convert_ubo: bool,
    pub no_sort: bool,
    pub alt_sort: bool,
    pub localhost: bool,
    pub comment_chars: &'a [String],
    pub backup: bool,
    pub keep_empty_lines: bool,
    pub ignore_dot_domains: bool,
    pub fix_typos: bool,
    pub quiet: bool,
    pub no_color: bool,
    pub dry_run: bool,
    /// Output changed files with --changed suffix
    pub output_changed: bool,
    /// Update timestamp in file header
    pub add_timestamp: bool,
}

/// Track changes made during sorting
#[derive(Default, Clone)]
pub struct SortChanges {
    pub typos_fixed: Vec<(String, String, String)>,       // (before, after, reason)
    pub domains_combined: Vec<(Vec<String>, String)>,     // (original rules, combined rule)
    pub has_text_merged: Vec<(Vec<String>, String)>,      // (original rules, merged rule)
    pub duplicates_removed: ahash::AHashSet<String>,      // removed duplicate rules (deduped)
    pub banned_domains_found: Vec<(String, String, String)>,  // (domain, rule, file)
}

use std::sync::Mutex;

/// Global change tracker for aggregating across files
pub static SORT_CHANGES: LazyLock<Mutex<SortChanges>> = 
    LazyLock::new(|| Mutex::new(SortChanges::default()));

/// Enable/disable change tracking (for --pr-show-changes)
pub static TRACK_CHANGES: std::sync::atomic::AtomicBool = 
    std::sync::atomic::AtomicBool::new(false);

// =============================================================================
// Banned Domain Checking
// =============================================================================

/// Extract domain from blocking rule for banned list check
#[inline]
fn extract_banned_domain(line: &str) -> Option<&str> {
    // Skip comments and cosmetic rules
    if line.starts_with('!') || line.contains("##") || line.contains("#@#") {
        return None;
    }
    
    // ||domain.com^$options or ||domain.com^ or ||domain.com
    let s = line.strip_prefix("||")?;
 
    // If domain= restriction exists, it's targeting specific sites, not blocking globally
    if line.contains("$domain=") || line.contains(",domain=") 
        || line.contains("$from=") || line.contains(",from=") 
    {
        return None;
    }
    
    // If there's a path (/) it's targeting specific resource, not whole domain
    if s.contains('/') {
        return None;
    }
    
    // Find end of domain (^ or $ or end of string)
    let end = s.find(['^', '$']).unwrap_or(s.len());
    
    // Check for path/pattern after ^ (like ^*.bmp or ^/path)
    // Only match if ^ is followed by nothing, $, or end of line
    if let Some(caret_pos) = s.find('^') {
        let after_caret = &s[caret_pos + 1..];
        if !after_caret.is_empty() && !after_caret.starts_with('$') {
            return None;
        }
    }

    if end > 0 {
        Some(&s[..end])
    } else {
        None
    }
}

/// Check if line matches a banned domain
#[inline]
pub fn check_banned_domain(line: &str, banned: &ahash::AHashSet<String>) -> Option<String> {
    // Quick check - if banned list is empty, skip
    if banned.is_empty() {
        return None;
    }

    // Check ||domain.com style rules
    if let Some(domain) = extract_banned_domain(line) {
        let domain_lower = domain.to_ascii_lowercase();
        if banned.contains(&domain_lower) {
            return Some(domain_lower);
        }
    }
    
    // Check plain domain lines (no || prefix, no # for cosmetic)
    let trimmed = line.trim();
    if !trimmed.starts_with('|') && !trimmed.contains('#') && !trimmed.starts_with('!') {
        let domain_lower = trimmed.to_ascii_lowercase();
        if banned.contains(&domain_lower) {
            return Some(domain_lower);
        }
    }
    
    None
}

/// Load banned domains from file
pub fn load_banned_list(path: &std::path::Path) -> io::Result<ahash::AHashSet<String>> {
    let content = fs::read_to_string(path)?;
    let domains: ahash::AHashSet<String> = content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('!') && !l.starts_with('#'))
        .map(|l| l.trim().to_ascii_lowercase())
        .collect();
    Ok(domains)
}

/// Run a change-tracking mutation only when TRACK_CHANGES is enabled.
/// Keeps call sites small and avoids repeating the load+lock boilerplate.
#[inline]
fn with_tracked_changes<F>(f: F)
where
    F: FnOnce(&mut SortChanges),
{
    if !TRACK_CHANGES.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if let Ok(mut changes) = SORT_CHANGES.lock() {
        f(&mut changes);
    }
}

/// Clear tracked changes (call before processing)
#[inline]
pub fn clear_tracked_changes() {
    if let Ok(mut changes) = SORT_CHANGES.lock() {
        *changes = SortChanges::default();
    }
}

// =============================================================================
// UBO Option Conversion
// =============================================================================

/// Convert uBO-specific options to standard ABP options
pub(crate) fn convert_ubo_options(options: Vec<String>) -> Vec<String> {
    options
        .into_iter()
        .map(|option| {
            if option.starts_with("from=") {
                option.replacen("from=", "domain=", 1)
            } else {
                UBO_CONVERSIONS
                    .get(option.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or(option)
            }
        })
        .collect()
}

/// Sort domains alphabetically, ignoring ~ prefix
pub(crate) fn sort_domains(domains: &mut [String]) {
    domains.sort_unstable_by(|a, b| {
        let (a_base, a_inv) = a.strip_prefix('~').map(|s| (s, true)).unwrap_or((a.as_str(), false));
        let (b_base, b_inv) = b.strip_prefix('~').map(|s| (s, true)).unwrap_or((b.as_str(), false));
        // base domain first; non-inverted before inverted when base is equal
        (a_base, a_inv).cmp(&(b_base, b_inv))
    });
}

// =============================================================================
// Filter Processing Functions
// =============================================================================

/// Remove unnecessary wildcards from filter text
pub(crate) fn remove_unnecessary_wildcards(filter_text: &str) -> String {
    let mut result = filter_text.to_string();
    let allowlist = result.starts_with("@@");

    if allowlist {
        result = result[2..].to_string();
    }

    let original_len = result.len();

    // Remove leading asterisks
    while result.len() > 1
        && result.starts_with('*')
        && !result[1..].starts_with('|')
        && !result[1..].starts_with('!')
    {
        result.remove(0);
    }

    // Remove trailing asterisks
    while result.len() > 1
        && result.ends_with('*')
        && !result[..result.len() - 1].ends_with('|')
        && !result[..result.len() - 1].ends_with(' ')
    {
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
pub(crate) fn filter_tidy(filter_in: &str, convert_ubo: bool) -> String {
    // Skip filters with regex values in options (contain =/.../ patterns)
    // ||example.com$removeparam=/^\\$ja=/
    // ||example.com$removeparam=/regex/
    if let Some(dollar_pos) = filter_in.rfind('$') {
        let options_part = &filter_in[dollar_pos..];
        if options_part.contains("=/") {
            return filter_in.to_string();
        }
    }

    // Fast path: no options to process (no $ in filter)
    if !filter_in.contains('$') {
        return remove_unnecessary_wildcards(filter_in);
    }

    let option_split = OPTION_PATTERN.captures(filter_in);

    match option_split {
        None => remove_unnecessary_wildcards(filter_in),
        Some(caps) => {
            let filter_text = remove_unnecessary_wildcards(&caps[1]);
            let option_list: Vec<String> = caps[2]
                .split(',')
                .map(|opt| {
                    // Only replace underscores in option name, not in value
                    if let Some(eq_pos) = opt.find('=') {
                        let name = opt[..eq_pos].to_ascii_lowercase().replace('_', "-");
                        let value = &opt[eq_pos..]; // Keep value as-is (preserve case and underscores)
                        format!("{}{}", name, value)
                    } else {
                        opt.to_ascii_lowercase().replace('_', "-")
                    }
                })
                .collect();

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
                if let Some(domains) = option.strip_prefix("domain=") {
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
                        || stripped.starts_with("ipaddress=")
                        || stripped.starts_with("method=")
                        || stripped.starts_with("denyallow=")
                        || stripped.starts_with("removeparam=")
                        || stripped.starts_with("urltransform=")
                        || stripped.starts_with("responseheader=")
                        || stripped.starts_with("sitekey=")
                        || stripped.starts_with("app=")
                        || stripped.starts_with("urlskip=")
                        || stripped.starts_with("uritransform=")
                        || stripped.starts_with("reason=")
                        || stripped.starts_with("addheader=")
                        || stripped.starts_with("referrerpolicy=")
                        || stripped.starts_with("cookie=")
                        || stripped.starts_with("removeheader=")
                        || stripped.starts_with("jsonprune=")
                        || stripped.starts_with("stealth=")
                        || stripped == "important"
                        || stripped == "media"
                        || stripped == "all";
                    if !is_known {
                        write_warning(&format!(
                            "Warning: The option \"{}\" used on the filter \"{}\" is not recognised by FOP",
                            option, filter_in
                        ));
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

            sorted_options.sort_unstable_by(|a, b| {
                let (a_base, a_inv) = a.strip_prefix('~').map(|s| (s, true)).unwrap_or((a.as_str(), false));
                let (b_base, b_inv) = b.strip_prefix('~').map(|s| (s, true)).unwrap_or((b.as_str(), false));
                (a_base, a_inv).cmp(&(b_base, b_inv))
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
pub(crate) fn element_tidy(domains: &str, separator: &str, selector: &str) -> String {
    let mut domains = domains.to_ascii_lowercase();

    // Sort domain names alphabetically
    if domains.contains(',') {
        let domain_list: Vec<&str> = domains.split(',').collect();
        let cap = domain_list.len();
        let mut valid_domains: Vec<String> = Vec::with_capacity(cap);
        let mut invalid_domains: Vec<String> = Vec::with_capacity(4);

        for d in &domain_list {
            let stripped = d.trim_start_matches('~');
            let len = stripped.len();
            let has_dot = stripped.contains('.');
            // Allow:
            // - * (wildcard for all domains)
            // - TLDs without dots (pl, de, com, org) - must be 2+ chars
            // - Regular domains with dots (example.com) - must be 4+ chars
            let is_valid = stripped == "*" || (!has_dot && len >= 2) || (has_dot && len >= 4);
            if !is_valid {
                invalid_domains.push((*d).to_string());
            } else {
                valid_domains.push((*d).to_string());
            }
        }

        if !invalid_domains.is_empty() {
            write_warning(&format!(
                "Removed invalid domain(s) from cosmetic rule: {} | Rule: {}{}{}",
                invalid_domains.join(", "),
                domains,
                separator,
                selector
            ));
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
        // Normalize scriptlet spacing (only simple args without quotes)
        if selector.starts_with("+js(") && !selector.contains('"') && !selector.contains('\'') {
            if let Some(start) = selector.find('(') {
                if let Some(end) = selector.rfind(')') {
                    let args = selector[start + 1..end].split(',').map(|a| a.trim()).collect::<Vec<_>>().join(", ");
                    return format!("{}{}{}{}", domains, separator, &selector[..start + 1], args) + &selector[end..];
                }
            }
        }
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
                    selector_without_strings =
                        selector_without_strings.replacen(&full_match, before, 1);
                    selector_only_strings.push_str(&string_val);
                } else {
                    break;
                }
            }
            None => break,
        }
    }

    // Clean up tree selectors
    // Skip normalization if selector contains pseudo-class functions (preserve original spacing)
    let skip_tree_normalize = selector.contains(":has(") || 
                              selector.contains(":not(") || 
                              selector.contains(":is(") || 
                              selector.contains(":where(");
    
    if !skip_tree_normalize {
        // Collect matches once to avoid cloning the whole selector just for iteration.
        let tree_caps: Vec<(String, String, String, String)> = TREE_SELECTOR
            .captures_iter(&selector)
            .map(|caps| {
                (
                    caps.get(0).unwrap().as_str().to_string(),
                    caps[1].to_string(),
                    caps[2].to_string(),
                    caps[3].to_string(),
                )
            })
            .collect();

        for (full_match, g1, g2, g3) in tree_caps {
            if selector_only_strings.contains(&full_match)
                || !selector_without_strings.contains(&full_match)
            {
                continue;
           }

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

            // Skip CSS attribute selector operator ~= (e.g., [rel~="sponsored"])
            if g2 == "~" && g3 == "=" {
                continue;
            }

            let replace_by = if g1 == "(" {
                format!("{} ", g2)
            } else {
                format!(" {} ", g2)
            };

            let replace_by = if replace_by == "   " {
                " ".to_string()
            } else {
                replace_by
            };

            selector = selector.replacen(&full_match, &format!("{}{}{}", g1, replace_by, g3), 1);
        }
    }

    // Remove unnecessary tags (asterisks)
    let removal_caps: Vec<(String, String, String, usize)> = REMOVAL_PATTERN
        .captures_iter(&selector)
        .filter_map(|caps| {
            let bc = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let untag = caps.get(2)?.as_str().to_string();
            let ac = caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            let end = caps.get(0).unwrap().end();
            Some((bc, untag, ac, end))
        })
        .collect();

    for (bc, untag_name, ac, match_end) in removal_caps {
        if selector_only_strings.contains(&untag_name)
            || !selector_without_strings.contains(&untag_name)
        {
            continue;
        }

        // Skip if this is a :not(-abp-contains...) pattern
        if ac == ":" && match_end <= selector.len() {
            let remaining = &selector[match_end..];
            if remaining.starts_with("-abp-contains")
                || remaining.starts_with("-abp-has")
                || remaining.starts_with("not(")
                || remaining.starts_with("has(")
            {
                continue;
            }
        }

        let old = format!("{}{}{}", bc, untag_name, ac);
        let new = format!("{}{}", bc, ac);
        selector = selector.replacen(&old, &new, 1);
    }

    // Make pseudo classes lowercase
    let pseudo_caps: Vec<(String, String)> = PSEUDO_PATTERN
        .captures_iter(&selector)
        .map(|caps| {
            (
                caps[1].to_string(),
                caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string(),
            )
        })
        .collect();

    for (pseudo_class, ac) in pseudo_caps {
        if selector_only_strings.contains(&pseudo_class)
            || !selector_without_strings.contains(&pseudo_class)
        {
            continue;
        }

        if UNICODE_SELECTOR.is_match(&selector_without_strings) {
            break;
        }

        let old = format!("{}{}", pseudo_class, ac);
        let new = format!("{}{}", pseudo_class.to_ascii_lowercase(), ac);
        selector = selector.replacen(&old, &new, 1);
    }

    // Remove markers and return complete rule
    let selector = &selector[1..selector.len() - 1];
    format!("{}{}{}", domains, separator, selector)
}

/// Escape special regex characters in plain text
#[inline]
fn escape_regex_chars(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match c {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Check if a :has-text() argument is a regex (starts and ends with /)
#[inline]
fn is_regex_arg(arg: &str) -> bool {
    arg.starts_with('/') && arg.ends_with('/')
}

/// Extract the regex content (without slashes) or escape plain text
#[inline]
fn normalize_has_text_arg(arg: &str) -> String {
    if is_regex_arg(arg) {
        arg[1..arg.len()-1].to_string()
    } else {
        escape_regex_chars(arg)
    }
}

/// Parse a selector to extract base selector and :has-text() argument
fn parse_has_text_selector(selector: &str) -> Option<(String, String, String)> {
    let caps = HAS_TEXT_PATTERN.captures(selector)?;
    Some((caps[1].to_string(), caps[2].to_string(), caps[3].to_string()))
}

/// Merge multiple :has-text() arguments into a single regex
fn merge_has_text_args(args: &[String]) -> String {
    if args.is_empty() {
        return String::new();
    }
    
    // Single rule - keep original format unchanged
    if args.len() == 1 {
        return args[0].clone();
    }

    // Multiple rules - combine into regex    
   let combined = args.iter()
        .map(|a| normalize_has_text_arg(a))
        .collect::<Vec<_>>()
        .join("|");
    
    format!("/{}/", combined)
}

/// Combine element rules with same domain and base selector but different :has-text() args
pub fn combine_has_text_rules(lines: Vec<String>) -> Vec<String> {
    let capacity = lines.len();
    let mut groups: AHashMap<(String, String, String), (usize, Vec<String>)> = AHashMap::with_capacity(capacity / 4);
    let mut order: Vec<(usize, String)> = Vec::with_capacity(capacity);
    let mut idx = 0;
    
    for line in lines {
        // Skip non-standard separators (#?#, #@#, #$#, etc.) and non-element rules
        if line.starts_with('!') 
            || line.starts_with('[') 
            || !line.contains("##") 
        {

            order.push((idx, line));
            idx += 1;
            continue;
        }
        
        let (domains, selector) = if let Some(pos) = line.find("##") {
            (&line[..pos], &line[pos+2..])
        } else {
            order.push((idx, line));
            idx += 1;
            continue;
        };
        
        if let Some((base, pseudo, arg)) = parse_has_text_selector(selector) {
            let key = (domains.to_string(), base, pseudo);
            let entry = groups.entry(key).or_insert_with(|| (idx, Vec::new()));
            entry.1.push(arg);
            // Only increment idx for first occurrence of this group
            if entry.1.len() == 1 {
                idx += 1;
            }
        } else {
            order.push((idx, line));
            idx += 1;
        }
    }
    
    // Add merged has-text rules with their original position
    for ((domains, base, pseudo), (pos, args)) in groups {
        // Only track if multiple rules were merged
        let was_merged = args.len() > 1;
               
        let merged_arg = merge_has_text_args(&args);
        let merged_rule = if domains.is_empty() {
            format!("##{}:{}({})", base, pseudo, merged_arg)
        } else {
            format!("{}##{}:{}({})", domains, base, pseudo, merged_arg)
        };
        
        // Track merge
        if was_merged {
            with_tracked_changes(|changes| {
                let originals: Vec<String> = args.iter().map(|arg| {
                    if domains.is_empty() {
                        format!("##{}:{}({})", base, pseudo, arg)
                    } else {
                        format!("{}##{}:{}({})", domains, base, pseudo, arg)
                    }
                }).collect();
                changes.has_text_merged.push((originals, merged_rule.clone()));
            });
        }
        
        order.push((pos, merged_rule));
    }
    // Sort by original position
    order.sort_by_key(|(pos, _)| *pos);
    
    let result: Vec<String> = order.into_iter().map(|(_, line)| line).collect();
    result
}

/// Combine filters with identical rules but different domains
fn combine_filters(
    mut uncombined: Vec<String>,
    domain_pattern: &Regex,
    separator: &str,
) -> Vec<String> {
    // No combining needed for single filter
    if uncombined.len() <= 1 {
        return uncombined;
    }
    let mut combined: Vec<String> = Vec::with_capacity(uncombined.len());

    for i in 0..uncombined.len() {
        let domains1 = domain_pattern.captures(&uncombined[i]);

        // Get domain info for current and next filter
        let (domain1_str, domains1_full) = if i + 1 < uncombined.len() {
            if let Some(ref caps) = domains1 {
                (
                    caps.get(1)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default(),
                    caps.get(0)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default(),
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

        let domain2_str = domains2
            .as_ref()
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");

        if domain2_str.is_empty() {
            combined.push(std::mem::take(&mut uncombined[i]));
            continue;
        }

        let domains2_full = domains2
            .as_ref()
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

        new_domains
            .sort_unstable_by(|a, b| {
                let (a_base, a_inv) = a.strip_prefix('~').map(|s| (s, true)).unwrap_or((a.as_str(), false));
                let (b_base, b_inv) = b.strip_prefix('~').map(|s| (s, true)).unwrap_or((b.as_str(), false));
                (a_base, a_inv).cmp(&(b_base, b_inv))
            });

        let new_domain_str = new_domains.join(separator);

        // Create the substitution pattern (full match with new domains)
        let domains_substitute = domains1_full.replace(&domain1_str, &new_domain_str);

        // Escape $ for regex replacement ($ is special in replacement strings)
        let escaped_substitute = if domains_substitute.contains('$') {
            domains_substitute.replace("$", "$$")
        } else {
            domains_substitute
        };

        // Modify the next filter to be the combined version
        // (using filter i as the base, replacing its domain pattern with the combined domains)

        let combined_filter = domain_pattern
            .replace(&uncombined[i], escaped_substitute.as_str())
            .to_string();
            
        // Track combination
        with_tracked_changes(|changes| {
            changes.domains_combined.push((
                vec![uncombined[i].clone(), uncombined[i + 1].clone()],
                combined_filter.clone(),
            ));
        });

        uncombined[i + 1] = combined_filter;

        // Don't add current filter to combined - it will be processed as part of next iteration
    }

    combined
}

// =============================================================================
// Main Sorting Function
// =============================================================================

/// Format Unix timestamp as "30 Jan 2026 08:31 UTC"
#[inline]
fn format_timestamp_utc(secs: u64) -> String {
    // Pre-allocate: "30 Jan 2026 08:31 UTC" = ~21 chars
    let mut result = String::with_capacity(24);
    const DAYS_IN_MONTH: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", 
                                 "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    
    let secs_per_day = 86400u64;
    let secs_per_hour = 3600u64;
    let secs_per_min = 60u64;
    
    let mut days = secs / secs_per_day;
    let remaining = secs % secs_per_day;
    let hours = remaining / secs_per_hour;
    let minutes = (remaining % secs_per_hour) / secs_per_min;
    
    // Start from 1970
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    
    let leap = is_leap_year(year);
    let mut month = 0usize;
    for (i, &d) in DAYS_IN_MONTH.iter().enumerate() {
        let days_in_month = if i == 1 && leap { 29 } else { d };
        if days < days_in_month {
            month = i;
            break;
        }
        days -= days_in_month;
    }
    
    let day = days + 1;
    use std::fmt::Write;
    let _ = write!(result, "{} {} {} {:02}:{:02} UTC", day, MONTHS[month], year, hours, minutes);
    result
}

#[inline]
fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Update timestamp in header line
#[inline]
fn update_timestamp_line(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("last modified:") || lower.contains("last updated:") {
        let prefix = if line.trim_start().starts_with('#') { "#" } else { "!" };
        let keyword = if lower.contains("last modified:") { "Last modified" } else { "Last updated" };
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        Some(format!("{} {}: {}", prefix, keyword, format_timestamp_utc(now)))
    } else {
        None
    }
}

/// Sort the sections of a filter file and save modifications
pub fn fop_sort(filename: &Path, config: &SortConfig) -> io::Result<Option<String>> {
    let temp_file = filename.with_extension("temp");
    const CHECK_LINES: usize = 10;

    // Skip empty or tiny files
    let metadata = fs::metadata(filename)?;
    if metadata.len() < 3 {
        return Ok(None);
    }

    // Read entire file into memory (avoids double-read for diff)
    let original_content = match fs::read(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot open {}: {}", filename.display(), e);
            return Ok(None);
        }
    };
    let reader = BufReader::new(Cursor::new(&original_content));
    let mut output = match File::create(&temp_file) {
        Ok(f) => BufWriter::with_capacity(64 * 1024, f),
        Err(e) => {
            eprintln!("Cannot create temp file for {}: {}", filename.display(), e);
            return Ok(None);
        }
    };

    let mut section: Vec<String> = Vec::with_capacity(2000);
    let mut lines_checked: usize = 1;
    let mut filter_lines: usize = 0;
    let mut element_lines: usize = 0;

    let write_filters = |section: &mut Vec<String>,
                         output: &mut BufWriter<File>,
                         element_lines: usize,
                         filter_lines: usize,
                         no_sort: bool,
                         alt_sort: bool,
                         localhost: bool|
     -> io::Result<()> {
        if section.is_empty() {
            return Ok(());
        }

        // Collect duplicates locally, merge once (reduces lock contention)
        let track_changes = TRACK_CHANGES.load(std::sync::atomic::Ordering::Relaxed);
        let mut dupes_local: HashSet<String> = HashSet::new();

        // Remove duplicates while preserving order if no_sort
        let mut unique: Vec<String> = if no_sort {
            let mut seen = HashSet::with_capacity(section.len());
            section
                .drain(..)
                .filter(|x| {
                    if !seen.insert(x.clone()) {
                        if track_changes {
                            dupes_local.insert(x.clone());
                        }
                        false
                    } else {
                        true
                    }
                })
                .collect()
        } else {
            let mut seen = HashSet::with_capacity(section.len());
            let unique: Vec<String> = section
                .drain(..)
                .filter(|x| {
                    if !seen.insert(x.clone()) {
                        if track_changes {
                            dupes_local.insert(x.clone());
                        }
                        false
                    } else {
                        true
                    }
                })
                .collect();
            unique
        };

        // Merge tracked duplicates into global changes once
        if track_changes && !dupes_local.is_empty() {
            if let Ok(mut changes) = SORT_CHANGES.lock() {
                changes.duplicates_removed.extend(dupes_local);
            }
        }

        if localhost {
            // Sort hosts file entries by domain
            if !no_sort {
                unique.sort_by_cached_key(|s| {
                    LOCALHOST_PATTERN
                        .captures(s)
                        .and_then(|c| c.get(2))
                        .map(|m| m.as_str().to_ascii_lowercase())
                        .unwrap_or_else(|| s.to_ascii_lowercase())
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
                unique.sort_by_cached_key(|s| pattern.replace(s, "").into_owned());
            }
            // Merge :has-text() rules first, then combine domains
            let merged = combine_has_text_rules(unique);
            let combined = combine_filters(merged, &ELEMENT_DOMAIN_PATTERN, ",");
            for filter in combined {
                writeln!(output, "{}", filter)?;
            }
        } else {
            // Sort blocking rules (unless no_sort)
            if !no_sort {
                unique.sort_by(|a, b| cmp_ascii_case_insensitive(a, b));
            }
            let combined = combine_filters(unique, &FILTER_DOMAIN_PATTERN, "|");
            for filter in combined {
                writeln!(output, "{}", filter)?;
            }
        }

        Ok(())
    };

    for line in reader.lines() {
        let line_owned = line?;
        let line = line_owned.trim();

        // Update timestamp if enabled and within first 10 lines
        let updated_line;
        let line = if config.add_timestamp && lines_checked <= CHECK_LINES {
            if let Some(updated) = update_timestamp_line(line) {
                updated_line = updated;
                updated_line.as_str()
            } else {
                line
            }
        } else {
            line
        };

        if line.is_empty() {
            if config.keep_empty_lines {
                if !section.is_empty() {
                    write_filters(
                        &mut section,
                        &mut output,
                        element_lines,
                        filter_lines,
                        config.no_sort,
                        config.alt_sort,
                        config.localhost,
                    )?;
                    lines_checked = 1;
                    filter_lines = 0;
                    element_lines = 0;
                }
                writeln!(output)?;
            }
            continue;
        }

        // Comments and special lines
        let is_comment = config.comment_chars.iter().any(|c| line.starts_with(c))
            || (config.localhost
                && line.starts_with('#')
                && !config.comment_chars.iter().any(|c| c == "#"));
        if is_comment
            || line.starts_with("%include")
            || (line.starts_with('[') && line.ends_with(']'))
        {
            if !section.is_empty() {
                write_filters(
                    &mut section,
                    &mut output,
                    element_lines,
                    filter_lines,
                    config.no_sort,
                    config.alt_sort,
                    config.localhost,
                )?;
                lines_checked = 1;
                filter_lines = 0;
                element_lines = 0;
            }
            writeln!(output, "{}", line)?;
            continue;
        }

        // Validate localhost entries when in localhost mode
        if config.localhost && !LOCALHOST_PATTERN.is_match(line) {
            write_warning(&format!("Removed invalid localhost entry: {}", line));
            continue;

        }

        // Skip filters less than 3 characters
        if line.len() < 3 {
            continue;
        }

        // [$path=/\/(dom|pro)/]rambler.ru##div[style^="order:"][style*="-1"]
        // AdGuard cosmetic rule modifiers - pass through unchanged
        if line.starts_with("[$") {
            section.push(line.to_string());
            continue;
        }

        // Handle regex domain rules (uBO) - pass through unchanged
        if REGEX_ELEMENT_PATTERN.is_match(line) {
            section.push(filter_tidy(line, config.convert_ubo));
            continue;
        }

        // Process element hiding rules
        let element_caps = if config.alt_sort {
            ELEMENT_PATTERN.captures(line)
        } else {
            FOPPY_ELEMENT_PATTERN.captures(line)
        };
        if let Some(caps) = element_caps {
            let domains = caps[1].to_ascii_lowercase();
            let separator = &caps[2];
            let selector = &caps[3];

            if lines_checked <= CHECK_LINES {
                element_lines += 1;
                lines_checked += 1;
            }

            let mut tidied = element_tidy(&domains, separator, selector);

            // Fix typos if enabled
            if config.fix_typos {
                let (fixed, fixes) = fop_typos::fix_all_typos(&tidied);
                if !fixes.is_empty() {
                with_tracked_changes(|changes| {
                    changes.typos_fixed.push((tidied.clone(), fixed.clone(), fixes.join(", ")));
                });
                    write_warning(&format!(
                        "Fixed typo: {} ? {} ({})",
                        tidied,
                        fixed,
                        fixes.join(", ")
                    ));
                    tidied = fixed;
                }
            }
            section.push(tidied);
            continue;
        }

        // Process blocking rules

        // Skip network rules without dot in domain
        if (line.starts_with("||") || line.starts_with('|'))
            && !SKIP_SCHEMES.iter().any(|s| line.starts_with(s))
        {
            if let Some(caps) = DOMAIN_EXTRACT_PATTERN.captures(line) {
                let domain = &caps[1];
                let is_ip = domain.starts_with('[') || IP_ADDRESS_PATTERN.is_match(domain);
                let has_wildcard = domain.contains('*');

                if !config.ignore_dot_domains
                    && !is_ip
                    && !has_wildcard
                    && !domain.contains('.')
                    && !domain.starts_with('~')
                {
                    write_warning(&format!(
                        "Skipped network rule without dot in domain: {} (domain: {})",
                        line, domain
                    ));
                    continue;
                }
            }
        }

        // Remove TLD-only patterns
        if is_tld_only(line) {
            write_warning(&format!("Removed overly broad TLD-only rule: {}", line));
            continue;
        }

        if lines_checked <= CHECK_LINES {
            filter_lines += 1;
            lines_checked += 1;
        }

        let mut tidied = filter_tidy(line, config.convert_ubo);

        // Fix typos if enabled (network rules)
        if config.fix_typos {
            let (fixed, fixes) = fop_typos::fix_all_typos(&tidied);
            if !fixes.is_empty() {
                    with_tracked_changes(|changes| {
                        changes.typos_fixed.push((tidied.clone(), fixed.clone(), fixes.join(", ")));
                    });
                write_warning(&format!(
                    "Fixed typo: {} ? {} ({})",
                    tidied, fixed, fixes.join(", ")
                ));
                tidied = fixed;
            }
        }
        section.push(tidied);
    }

    // Write remaining filters
    if !section.is_empty() {
        write_filters(
            &mut section,
            &mut output,
            element_lines,
            filter_lines,
            config.no_sort,
            config.alt_sort,
            config.localhost,
        )?;
    }

    drop(output);

    // Compare files and replace if different
    let new_content = fs::read(&temp_file)?;

    if original_content != new_content {
        if config.dry_run {
            if config.output_changed {
                // Write to filename--changed.ext
                let stem = filename.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
                let ext = filename.extension().and_then(|e| e.to_str()).unwrap_or("txt");
                let changed_filename = filename.with_file_name(format!("{}--changed.{}", stem, ext));
                
                fs::write(&changed_filename, &new_content)?;
                fs::remove_file(&temp_file)?;
                
                if !config.quiet {
                    println!("Changed file written to: {}", changed_filename.display());
                }
                
                return Ok(Some(format!("Modified: {} -> {}", filename.display(), changed_filename.display())));
            }
            // Generate unified diff
            let original_str = String::from_utf8_lossy(&original_content);
            let new_str = String::from_utf8_lossy(&new_content);

            let diff = similar::TextDiff::from_lines(&*original_str, &*new_str)
                .unified_diff()
                .header(
                    &format!("a/{}", filename.display()),
                    &format!("b/{}", filename.display()),
                )
                .to_string();

            fs::remove_file(&temp_file)?;
            return Ok(Some(diff));
        } else {
            // Create backup if requested
            if config.backup {
                let backup_file = filename.with_extension("backup");
                fs::copy(filename, &backup_file)?;
            }
            fs::rename(&temp_file, filename)?;
            if !config.quiet {
                if config.no_color {
                    let _ = writeln!(std::io::stdout().lock(), "Sorted: {}", filename.display());
                } else {
                    println!("{} {}", "Sorted:".bold(), filename.display());
                }
            }
        }
    } else {
        fs::remove_file(&temp_file)?;
    }

    Ok(None)
}
