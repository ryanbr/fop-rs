//! Filter sorting and tidying logic
//!
//! Copyright (C) 2025 FanboyNZ (FOP Rust)
//! https://github.com/ryanbr/fop-rs
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

#![allow(clippy::write_with_newline)]

use std::borrow::Cow;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::io::Cursor;
use std::path::Path;

use crate::fop_datestamp::{update_timestamp_line, update_version_line};


use owo_colors::OwoColorize;
use ahash::AHashSet as HashSet;
use ahash::AHashMap;
use regex::Regex;
use std::cmp::Ordering;

use crate::{
    write_warning, ADGUARD_ELEMENT_DOMAIN_PATTERN, ADGUARD_ELEMENT_PATTERN,
    ATTRIBUTE_VALUE_PATTERN, ELEMENT_DOMAIN_PATTERN,
    ELEMENT_PATTERN, FILTER_DOMAIN_PATTERN, FOPPY_ELEMENT_DOMAIN_PATTERN, FOPPY_ELEMENT_PATTERN,
    KNOWN_OPTIONS,
    PSEUDO_PATTERN, REGEX_ELEMENT_PATTERN, REMOVAL_PATTERN, TREE_SELECTOR,
    UBO_CONVERSIONS, UNICODE_SELECTOR,
};

use crate::fop_typos;

/// Safe values for trusted -> non-trusted scriptlet conversion (case-insensitive)
static TRUSTED_SAFE_VALUES: LazyLock<ahash::AHashSet<&'static str>> = LazyLock::new(|| {
    [
        "accept", "reject", "accepted", "rejected", "notaccepted",
        "allow", "disallow", "deny", "allowed", "denied",
        "approved", "disapproved", "checked", "unchecked",
        "dismiss", "dismissed", "enable", "disable", "enabled", "disabled",
        "essential", "nonessential", "forbidden", "forever",
        "hide", "hidden", "necessary", "required",
        "ok", "on", "off", "true", "t", "false", "f",
        "yes", "y", "no", "n", "all", "none", "functional",
        "granted", "done", "decline", "declined",
        "closed", "next", "mandatory", "disagree", "agree",
        "1", "0", "emptyarr", "emptyobj",
    ].into_iter().collect()
});

/// Check if a value is safe for non-trusted scriptlets
#[inline]
fn is_safe_scriptlet_value(value: &str) -> bool {
    TRUSTED_SAFE_VALUES.contains(value.to_ascii_lowercase().as_str())
        || value.parse::<u32>().is_ok_and(|n| n <= 32767)
}

/// Trusted scriptlet prefixes that can be converted to non-trusted
const TRUSTED_SCRIPTLETS: &[(&str, &str)] = &[
    ("trusted-set-cookie", "set-cookie"),
    ("trusted-set-local-storage-item", "set-local-storage-item"),
    ("trusted-set-session-storage-item", "set-session-storage-item"),
];

/// Convert trusted scriptlet to non-trusted if value is safe.
/// Handles both uBO `+js(trusted-set-cookie, name, value)` and
/// AdGuard `//scriptlet('trusted-set-cookie', 'name', 'value')` formats.
fn convert_trusted_scriptlet(line: &str) -> Option<String> {
    for &(trusted, non_trusted) in TRUSTED_SCRIPTLETS {
        if !line.contains(trusted) {
            continue;
        }

        // uBO format: +js(trusted-set-cookie, name, value)
        if let Some(js_pos) = line.find("+js(") {
            let args_start = js_pos + 4;
            let args_end = line.rfind(')')?;
            let args = &line[args_start..args_end];
            // Exactly name, cookie and value, split where uBO splits: a value
            // followed by further arguments, or holding a `\,`, is not one
            // is_safe_scriptlet_value can vouch for.
            let parts: Vec<&str> = split_unescaped_commas(args).into_iter().map(str::trim).collect();
            if parts.len() == 3 && parts[0] == trusted {
                let value = parts[2].trim();
                if is_safe_scriptlet_value(value) {
                    let converted = format!("{}+js({}, {}, {}){}", &line[..js_pos], non_trusted, parts[1], value, &line[args_end + 1..]);
                    return Some(converted);
                }
            }
            return None;
        }

        // AdGuard format: //scriptlet('trusted-set-cookie', 'name', 'value')
        if let Some(sc_pos) = line.find("//scriptlet(") {
            let args_start = sc_pos + 12;
            let args_end = line.rfind(')')?;
            let args = &line[args_start..args_end];
            let parts: Vec<&str> = args.splitn(3, ',').map(|s| s.trim()).collect();
            if parts.len() >= 3 {
                // Strip quotes for comparison
                let scriptlet_name = parts[0].trim_matches('\'').trim_matches('"');
                if scriptlet_name == trusted {
                    let value = parts[2].trim().trim_matches('\'').trim_matches('"');
                    if is_safe_scriptlet_value(value) {
                        let quoted_trusted = format!("'{}'", trusted);
                        let quoted_non_trusted = format!("'{}'", non_trusted);
                        let converted = line.replacen(&quoted_trusted, &quoted_non_trusted, 1);
                        return Some(converted);
                    }
                }
            }
            return None;
        }
    }
    None
}

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

/// Where a hosts entry's address ends, if the line is one.
///
/// A hosts entry is an IP address, whitespace, then a hostname. Both halves
/// matter: `0.0.0.0` and `127.0.0.1` are what a blocklist null-routes with,
/// but a hosts file's own preamble is not written in either. StevenBlack's
/// opens with `255.255.255.255 broadcasthost` and eight IPv6 lines -- `::1
/// localhost`, `fe00::0 ip6-localnet`, `ff02::3 ip6-allhosts` -- and matching
/// two addresses by name mangled every one of them, and left the file
/// unrecognised besides, since recognition asks that every rule be an entry.
///
/// The sort calls this on every rule it writes, so a filter rule has to fall
/// out cheaply. An address is at most 45 characters and holds only hex digits,
/// `.` and `:`, so the scan for the separating whitespace doubles as the test
/// that what precedes it could be an address at all: `example.com##div > p`
/// stops on the `m`. Only what survives that is parsed.
#[inline]
pub(crate) fn hosts_entry_split(line: &str) -> Option<usize> {
    const MAX_ADDR: usize = 45;
    let bytes = line.as_bytes();
    let limit = bytes.len().min(MAX_ADDR + 1);
    let mut end = 0;
    while end < limit {
        let b = bytes[end];
        if b == b' ' || b == b'\t' {
            break;
        }
        if !(b.is_ascii_hexdigit() || b == b'.' || b == b':') {
            return None;
        }
        end += 1;
    }
    // No separator inside the window: either the line is all address and has
    // no host, or it is far too long to be one.
    if end == 0 || end >= limit {
        return None;
    }
    if line[..end].parse::<std::net::IpAddr>().is_err() {
        return None;
    }
    (!line[end..].trim_start().is_empty()).then_some(end)
}

/// Whether a line is a hosts file entry: `IP<space>host`.
#[inline]
pub(crate) fn is_localhost_entry(line: &str) -> bool {
    hosts_entry_split(line).is_some()
}

/// Whether a file reads as a hosts file rather than a filter list.
///
/// Hosts files ship beside filter lists -- listefr carries `hosts.txt` next to
/// `liste_fr.txt` -- and they need `#` read as a comment and entries ordered by
/// host, which is what `--localhost` turns on. Left to the filter-list rules a
/// hosts file still sorts, so nothing looks wrong, but `#` stops being a
/// comment: a `####...` banner parses as `##` plus an id selector, so the run
/// of entries under it is no longer a section of its own.
///
/// Every rule has to be an entry, not merely the first few. Sampling the head
/// would call a file a hosts file on the strength of its opening lines, and in
/// a file taken for one `#` starts a comment -- which would turn every generic
/// `##.ad` rule below the sample into a comment. The scan is what licenses
/// that reading, so it reads the whole file.
///
/// It stays cheap because a filter list disqualifies itself on its first rule,
/// which is within a few lines of the top; only a file that really is all
/// entries is read to the end. The bytes are the caller's, already in memory,
/// so no I/O is repeated.
///
/// This decides formatting, never deletion. `--localhost` drops a line that is
/// not an entry, and a guess must not do that; see the sort loop.
pub(crate) fn looks_like_hosts_file(content: &[u8]) -> bool {
    let mut entries = 0usize;
    for raw in content.split(|&b| b == b'\n') {
        let Ok(line) = std::str::from_utf8(raw) else { return false };
        let line = line.trim();
        // A hosts file comments with `#`, but so few of the `#` spellings are
        // comments that the character cannot be skipped on sight: `##.ad` is a
        // generic hide rule, and `#@#`, `#?#`, `#$#` and `#%#` are rules too.
        // Skipping every `#` would read a list of generic rules as a file with
        // no rules at all, and a handful of entries anywhere in it would then
        // carry the whole file -- whose rules this would go on to comment out.
        // Only a `#` run (a banner) and `#` before whitespace are comments.
        //
        // `!` and `[Adblock Plus 2.0]` are a filter list's own comment and
        // header; they are skipped rather than counted against a file so a
        // hosts file carrying either is still recognised.
        let hash_comment = line.starts_with('#')
            && (is_plain_comment(line) || line.bytes().all(|b| b == b'#'));
        if line.is_empty()
            || hash_comment
            || line.starts_with('!')
            || (line.starts_with('[') && line.ends_with(']'))
        {
            continue;
        }
        if !is_localhost_entry(line) {
            return false;
        }
        entries += 1;
    }
    entries > 0
}

/// The host a hosts entry names, for ordering. Falls back to the whole line,
/// which is what a line that is not an entry sorts on.
#[inline]
pub(crate) fn localhost_domain(line: &str) -> &str {
    match hosts_entry_split(line) {
        Some(end) => line[end..].trim_start(),
        None => line.trim_start(),
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
    /// Convert ABP extended selectors to uBO format
    pub abp_convert: bool,
    /// Promote a `:has-text()` rule's separator to AdGuard's spelling —
    /// `##` -> `#?#`, `#@#` -> `#@?#`. Independent of `abp_convert`, off by
    /// default.
    pub adguard_convert: bool,
    /// Convert trusted-set-cookie/storage to non-trusted when value is safe
    pub convert_trusted: bool,
    /// Parse AdGuard extended CSS selectors (#$?# and #@$?#)
    pub parse_adguard: bool,
    pub localhost: bool,
    pub comment_chars: &'a [String],
    pub backup: bool,
    pub keep_empty_lines: bool,
    pub ignore_dot_domains: bool,
    pub fix_typos: bool,
    /// Keep rules shorter than the three-character floor instead of dropping
    /// them as malformed
    pub ignore_line_minimum: bool,
    pub quiet: bool,
    pub no_color: bool,
    pub dry_run: bool,
    /// Output changed files with --changed suffix
    pub output_changed: bool,
    /// Update timestamp in file header
    pub add_timestamp: bool,
    /// Timing the sort (`--benchmark`): nothing reads the result
    pub benchmark: bool,
}

/// Track changes made during sorting
#[derive(Default, Clone)]
pub struct SortChanges {
    pub typos_fixed: Vec<(String, String, String)>,       // (before, after, reason)
    pub domains_combined: Vec<(Vec<String>, String)>,     // (original rules, combined rule), the first PR_CHANGES_SHOWN
    pub domains_combined_count: usize,                    // every merge step, recorded in full or not
    pub has_text_merged: Vec<(Vec<String>, String)>,      // (original rules, merged rule)
    pub duplicates_removed: ahash::AHashSet<String>,      // removed duplicate rules (deduped)
    pub banned_domains_found: Vec<(String, String, String)>,  // (domain, rule, file)
}

use std::sync::Mutex;

/// Global change tracker for aggregating across files
pub static SORT_CHANGES: LazyLock<Mutex<SortChanges>> = 
    LazyLock::new(|| Mutex::new(SortChanges::default()));

/// Items of each kind the PR description lists; the rest it only counts.
pub const PR_CHANGES_SHOWN: usize = 40;

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
        // Strip >> marker for comparison so example.com and example.com>> sort together
        let a_name = a_base.strip_suffix(">>").unwrap_or(a_base);
        let b_name = b_base.strip_suffix(">>").unwrap_or(b_base);
        let a_has_marker = a_base.ends_with(">>");
        let b_has_marker = b_base.ends_with(">>");
        // base domain first; non-inverted before inverted; non-ancestor before ancestor
        (a_name, a_inv, a_has_marker).cmp(&(b_name, b_inv, b_has_marker))
    });
}

// =============================================================================
// Filter Processing Functions
// =============================================================================

/// Remove unnecessary wildcards from filter text
pub(crate) fn remove_unnecessary_wildcards(filter_text: &str) -> Cow<'_, str> {
    // Fast path: no wildcards to process
    if !(filter_text.starts_with('*') || filter_text.ends_with('*')
        || filter_text.starts_with("@@") && filter_text.get(2..3) == Some("*"))
    {
        return Cow::Borrowed(filter_text);
    }

    let mut result = filter_text.to_string();
    let allowlist = result.starts_with("@@");

    if allowlist {
        result = result[2..].to_string();
    }

    let original_len = result.len();

    // Remove leading asterisks
    let skip = result.bytes()
        .take_while(|&b| b == b'*')
        .count()
        .min(result.len().saturating_sub(1));
    // A leading `*` is only redundant when a pattern follows it.
    //   `|` / `!` — existing guards, left exactly as written.
    //   `$` — there is no pattern, only options, and `*` is standing in as the
    //         pattern. Reached when the whole rule arrives here rather than
    //         just its pattern, i.e. when the options did not match
    //         OPTION_PATTERN (`$csp=` and friends, whose values contain
    //         spaces). Keep one `*`; collapse a repeat, which is just untidy.
    if skip > 0 {
        match result.as_bytes().get(skip) {
            Some(b'|' | b'!') => {}
            Some(b'$') => {
                if skip > 1 {
                    result = result[skip - 1..].to_string();
                }
            }
            _ => result = result[skip..].to_string(),
        }
    }

    // Remove trailing asterisks.
    //
    // Not when the whole rule arrived here rather than just its pattern, as it
    // does when the options did not match OPTION_PATTERN -- `$csp=` and
    // friends, whose values hold spaces. Then the last `*` closes the final
    // option's value, not the pattern: `*$csp=script-src *,domain=isohunt.*`
    // was written back as `domain=isohunt.`, a host with a trailing dot that
    // matches nothing, and `*$csp=script-src *,domain=torrentproject2.*` lost
    // the only domain it had. The `ends_with(' ')` guard below caught only the
    // narrower `$csp=script-src *` with nothing after it.
    let carries_options = find_option_separator(&result).is_some();
    while !carries_options
        && result.len() > 1
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

    Cow::Owned(result)
}

/// Is the `$` at `i` part of a cosmetic or HTML-filtering separator rather
/// than the start of filter options?
///
/// Covers `#$#`, `#@$#`, `#$?#`, `#@$?#` (AdGuard CSS injection and extended
/// CSS) and `$$` / `$@$` (AdGuard HTML filtering) — the same set treated as
/// extended syntax by `element_tidy`.
#[inline]
fn dollar_is_element_separator(bytes: &[u8], i: usize) -> bool {
    // `$$` / `$@$`, matched from either `$`.
    if bytes.get(i + 1) == Some(&b'$') || (i > 0 && bytes[i - 1] == b'$') {
        return true;
    }
    if bytes.get(i + 1) == Some(&b'@') && bytes.get(i + 2) == Some(&b'$') {
        return true;
    }
    if i >= 2 && bytes[i - 1] == b'@' && bytes[i - 2] == b'$' {
        return true;
    }
    // `#$#` / `#@$#` / `#$?#` / `#@$?#`
    let after_hash = i > 0
        && (bytes[i - 1] == b'#' || (bytes[i - 1] == b'@' && i >= 2 && bytes[i - 2] == b'#'));
    let before_hash = bytes.get(i + 1) == Some(&b'#')
        || (bytes.get(i + 1) == Some(&b'?') && bytes.get(i + 2) == Some(&b'#'));
    after_hash && before_hash
}

/// Find the option separator `$` position, skipping escaped `\$` in values.
///
/// A `$` that forms part of a cosmetic separator is not an option separator:
/// reading the `$` in `#$#` as one made the whole selector and stylesheet body
/// look like an option list, and `filter_tidy`'s `$option.option` typo fix then
/// rewrote every `.` in it to `,`, turning `div.ad` into `div,ad`.
#[inline]
fn find_option_separator(filter: &str) -> Option<usize> {
    let bytes = filter.as_bytes();
    let mut i = filter.len();
    while i > 0 {
        i -= 1;
        if bytes[i] == b'$'
            && (i == 0 || bytes[i - 1] != b'\\')
            && !dollar_is_element_separator(bytes, i)
        {
            return Some(i);
        }
    }
    None
}

/// Split on the commas that separate options, not the escaped ones.
///
/// `\,` is a comma inside a value: `$permissions=sync-xhr=()\,camera=()` is one
/// option. Splitting on every comma cut it in two, so the sorter reordered the
/// halves into `$camera=(),permissions=sync-xhr=()\` -- a dangling backslash
/// and a broken rule on every sort -- and the rule checker judged `camera=()`
/// as an option of its own, called it unknown, and deleted the line.
///
/// Backslashes are counted, as uBO's argument parser counts them: an odd run
/// escapes the comma, an even one is escaped backslashes before a real
/// separator, so `a\\,b` is two parts (`a\\` and `b`).
#[inline]
pub(crate) fn split_unescaped_commas(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts = Vec::with_capacity(4);
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        // `,` is ASCII, so every split point is a char boundary.
        if b == b',' && bytes[..i].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 0 {
            parts.push(&s[start..i]);
            start = i + 1;
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Split filter options on commas, keeping values intact for options like
/// `jsonprune=`/`xmlprune=` where commas are part of the value syntax.
#[inline]
pub(crate) fn split_filter_options(options: &str) -> Vec<&str> {
    let parts: Vec<&str> = split_unescaped_commas(options);
    if parts.len() <= 1 {
        return parts;
    }
    let mut result: Vec<&str> = Vec::with_capacity(parts.len());
    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];
        // jsonprune/xmlprune values use commas in path syntax — keep them joined
        if part.starts_with("jsonprune=") || part.starts_with("jsonprune\\=")
            || part.starts_with("xmlprune=") || part.starts_with("xmlprune\\=") {
            // Find the start and end byte offsets within the original string to return a single slice
            let start_ptr = part.as_ptr() as usize - options.as_ptr() as usize;
            i += 1;
            let mut end_ptr = start_ptr + part.len();
            while i < parts.len() && !parts[i].contains('=')
                && !KNOWN_OPTIONS.contains(parts[i].trim_start_matches('~'))
                && parts[i] != "important" && parts[i] != "media" && parts[i] != "all"
            {
                // +1 for the comma separator
                end_ptr += 1 + parts[i].len();
                i += 1;
            }
            result.push(&options[start_ptr..end_ptr]);
        } else {
            result.push(part);
            i += 1;
        }
    }
    result
}

/// `remove_unnecessary_wildcards` clears a pattern that reduces to `*`. Put a
/// single `*` back — but only if the author wrote one.
///
/// Both spellings of an options-only rule are valid, so fop keeps whichever is
/// used: `*$ping,third-party` stays as written (the `*` is the pattern, and the
/// pattern-less form is not accepted by every consumer of these lists), and a
/// bare `$ping,third-party` stays bare rather than being rewritten. Restoring
/// unconditionally would have normalised one form to the other and churned
/// every existing options-only rule in the lists.
///
/// Also stops a rule of nothing but wildcards (`***`) becoming a blank line.
#[inline]
fn restore_cleared_wildcard(original: &str, tidied: String) -> String {
    if (tidied.is_empty() || tidied == "@@") && original.contains('*') {
        format!("{}*", tidied)
    } else {
        tidied
    }
}

/// Whether the rule carries an option whose value may legitimately hold spaces.
///
/// Header values carry them as a matter of course --
/// `content-type:text/html; charset=utf-8` -- as do `permissions=` policies,
/// `addheader=` cookie attributes and AdGuard's `extension=`, which names a
/// userscript exactly: `$extension='AdGuard Assistant'` stripped to
/// `'AdGuardAssistant'` names nothing, and the exception silently stopped
/// applying -- seven rules in AdGuard's allowlist. Stripping whitespace from a
/// rule bearing one changes what it matches.
///
/// `reason=` is free text, and `uritransform=`, `urltransform=` and
/// `ipaddress=` take a regex, where a space is part of what matches:
/// `ipaddress=/^1\.2\.3\.4 $/` stripped is a different address. Five rules in
/// uAssets' badware.txt had their `reason=` text run together before these
/// were listed; the other three carry no space in the lists today.
///
/// Matched by scanning for the name and testing the byte before it, rather than
/// by building `"$name"` and `",name"` to search for: that allocated two
/// Strings per name, one pair for each, to test for one byte.
fn carries_space_valued_option(filter_in: &str) -> bool {
    const SPACE_VALUED: [&str; 16] = [
        "csp=", "replace=", "urlskip=", "removeparam=", "jsonprune=", "xmlprune=",
        "header=", "responseheader=", "requestheader=", "permissions=", "addheader=",
        "extension=",
        "reason=", "uritransform=", "urltransform=", "ipaddress=",
    ];
    let bytes = filter_in.as_bytes();
    SPACE_VALUED.iter().any(|name| {
        filter_in
            .match_indices(name)
            // Only as an option, not as text inside a pattern.
            .any(|(at, _)| at > 0 && matches!(bytes[at - 1], b'$' | b','))
    })
}

/// Whether a network rule's pattern is a regex: `/.../`, before any option list.
///
/// A space in a regex is part of what it matches -- `[^&=? ]` excludes spaces,
/// `[^&=?]` does not -- so stripping it changes the rule. The old test looked
/// at the whole line, so it caught only a regex with no options: one carrying
/// `$script,third-party` ended in `third-party`, was not recognised, and had
/// its character class rewritten. The same held for every `@@/.../` exception.
#[inline]
fn has_regex_pattern(filter: &str) -> bool {
    let body = filter.strip_prefix("@@").unwrap_or(filter);
    let pattern = crate::fop_rules::split_options(body).map_or(body, |(pattern, _)| pattern);
    pattern.len() > 1 && pattern.starts_with('/') && pattern.ends_with('/')
}

/// Sort and clean filter options.
pub(crate) fn filter_tidy(filter_in: &str, convert_ubo: bool) -> String {
    // Skip filters with regex values in options (contain =/.../ patterns)
    // ||example.com$removeparam=/^\\$ja=/
    // ||example.com$removeparam=/regex/

    // Element rules have no option list, so nothing below that rewrites options
    // may touch them. Computed before the typo fix rather than after: reading a
    // cosmetic separator's `$` as an option separator made the selector look
    // like options, and the typo fix rewrote every `.` in it to `,`.
    // `#@?#` and `$@$` were missing, so AdGuard exception rules in those forms
    // had their selector whitespace stripped: `$@$script[tag-content="ad
    // config"]` became `"adconfig"` while the identical `$$` rule was left
    // alone. Deliberately a loose substring test — erring towards not touching
    // something that might be cosmetic.
    const COSMETIC_SEPARATORS: [&str; 10] =
        ["##", "#@#", "#?#", "#@?#", "#$#", "#@$#", "#%#", "#@%#", "#$?#", "#@$?#"];
    let is_element_rule = (filter_in.contains('#')
        && COSMETIC_SEPARATORS.iter().any(|s| filter_in.contains(s)))
        || filter_in.contains("$$")
        || filter_in.contains("$@$");

    // Fix typo: $option.option -> $option,option (before pattern matching)
    //
    // Network rules only, judged the way ABP, uBO and AdGuard all parse a
    // line: a cosmetic separator anywhere makes it cosmetic. A narrower,
    // anchored test was tried so that a network rule carrying `##` in its URL
    // path would still be repaired -- but no such rule exists (a URL fragment
    // is never part of a request, so it could not match; no engine would read
    // it as a network rule; and none appears in 2.2M lines of real lists). The
    // anchored test missed a cosmetic domain list mixing plain and regex
    // domains, where a regex's `$/` reads as an option marker, and rewrote
    // uAssets' `+js(acs, Math.random, ...)` to `Math,random`.
    let filter_in: Cow<str> = match find_option_separator(filter_in) {
        Some(dollar_pos) if !is_element_rule => {
            let (base, opts) = filter_in.split_at(dollar_pos);
            if !opts.contains('=') && opts.contains('.') {
                Cow::Owned(format!("{}{}", base, opts.replace('.', ",")))
            } else {
                Cow::Borrowed(filter_in)
            }
        }
        _ => Cow::Borrowed(filter_in),
    };
    let filter_in = filter_in.as_ref();

    // Remove errant spaces from network filters only
    // Skip: element rules, regex patterns, and options with legitimate spaces
    // Header values carry spaces as a matter of course --
    // `content-type:text/html; charset=utf-8` -- as do `permissions=` policies,
    // so stripping whitespace from a rule bearing one changes what it matches.
    // Ordered cheapest test first. Whether the rule carries an option whose
    // value may hold spaces only matters if it holds whitespace at all, and
    // almost none do -- so the scan below runs on a handful of rules rather
    // than on every one.
    let has_whitespace = filter_in.bytes().any(|b| b == b' ' || b == b'\t');
    let filter_in: Cow<str> = if has_whitespace {
        if !is_element_rule
            && !has_regex_pattern(filter_in)
            && !carries_space_valued_option(filter_in)
        {
            Cow::Owned(filter_in.split_whitespace().collect::<String>())
        } else {
            Cow::Borrowed(filter_in)
        }
    } else {
        Cow::Borrowed(filter_in)
    };
    let filter_in = filter_in.as_ref();

    if let Some(dollar_pos) = find_option_separator(filter_in) {
        let options_part = &filter_in[dollar_pos..];
        if options_part.contains("=/") {
            return filter_in.to_string();
        }
    }

    // Fast path: no options to process (no $ in filter)
    if !filter_in.contains('$') {
        return restore_cleared_wildcard(
            filter_in,
            remove_unnecessary_wildcards(filter_in).into_owned(),
        );
    }

    // A cosmetic rule has no option list: what follows its separator is a
    // selector. AdGuard's HTML filters (`$$`, `$@$`) reach here in the default
    // mode, where reading the selector as options warned that `amp-consent` in
    // `...$$amp-consent` is not an option FOP knows. Nothing below rewrites
    // such a line -- only the warning was wrong -- so it is returned as it
    // stands, in every mode rather than only under --parse-adguard.
    if is_element_rule {
        return filter_in.to_string();
    }

    // `OPTION_PATTERN` by scan. The regex leads with `.*` and backtracks over
    // every `$` in the line, which puts the regex crate on its bounded
    // backtracker -- the hottest function in a profile of a real sort once the
    // element patterns were gated. The two agree on all 2,619,918 lines of
    // four corpora; see `split_options_as_pattern` for the one shape where
    // agreeing took care.
    let option_split = crate::fop_rules::split_options_as_pattern(filter_in);

    match option_split {
        None => restore_cleared_wildcard(
            filter_in,
            remove_unnecessary_wildcards(filter_in).into_owned(),
        ),
        Some((pattern, options)) => {
            // A rule with options but no pattern is spelled `*$opts`, and the
            // `*` IS the pattern. Tidying reduces it to nothing, producing the
            // pattern-less `$opts` form, which not every consumer of these
            // lists accepts — so put a single `*` back. Also collapses `**$opts`
            // to one wildcard, since the repeat is just untidy.
            let filter_text = restore_cleared_wildcard(
                pattern,
                remove_unnecessary_wildcards(pattern).into_owned(),
            );
            let option_list: Vec<String> = split_filter_options(options)
                .into_iter()
                .map(|opt| {
                    // Only replace underscores in option name, not in value
                    if let Some(eq_pos) = opt.find('=') {
                        let name = opt[..eq_pos].to_ascii_lowercase().replace('_', "-");
                        let value = &opt[eq_pos..]; // Keep value as-is (preserve case and underscores)
                        format!("{}{}", name, value)
                    } else if !opt.is_empty() && opt.bytes().all(|b| b == b'_') {
                        // AdGuard's noop modifier is a run of underscores and
                        // nothing else, used to keep a long rule readable:
                        // `$script,third-party,denyallow=...,_____,domain=...`.
                        // The `_` -> `-` normalisation exists for option names
                        // like `redirect_rule`; applied here it produced
                        // `-----`, which is not an option at all. All 27 in
                        // AdguardFilters were being rewritten that way.
                        opt.to_string()
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
                    domain_list.extend(
                        domains.split('|')
                            .map(|d| d.trim())                                    // Remove spaces
                            .map(|d| d.trim_start_matches(['=', '.', '&', '@', ',', '#', '$']))
                            .filter(|d| !d.is_empty())
                            .map(String::from)
                    );
                    remove_entries.insert(option.clone());
                } else {
                    let stripped = option.trim_start_matches('~');
                    let is_known = crate::is_known_option_in(stripped, filter_in.starts_with("@@"));
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

/// Whether the `:` at `colon` is something other than a pseudo-class, and so
/// must keep its case.
///
/// Two ways that happens:
///
/// * It is escaped. `#js\:cookies\:barInitWrapper` is one id containing literal
///   colons, and ids are case-sensitive. An even run of backslashes is itself
///   escaped and leaves the colon live, so the run is counted.
/// * It opens a regex group -- `(?:` non-capturing, `(?i:` / `(?-is:` inline
///   flags -- inside a `:contains(/.../)` argument. A pseudo-class never has a
///   `?` there: walking back over flag letters from `.mix:HOVER` lands on `.`.
#[inline]
fn not_a_pseudo_class(bytes: &[u8], colon: usize) -> bool {
    let mut i = colon;
    let mut backslashes = 0;
    while i > 0 && bytes[i - 1] == b'\\' {
        backslashes += 1;
        i -= 1;
    }
    if backslashes % 2 == 1 {
        return true;
    }
    let mut i = colon;
    while i > 0 && matches!(bytes[i - 1], b'i' | b'm' | b's' | b'x' | b'u' | b'U' | b'R' | b'-') {
        i -= 1;
    }
    i > 0 && bytes[i - 1] == b'?'
}

/// Pseudo-classes whose argument is not a selector.
///
/// `:contains(` is AdGuard and ABP's name for `:has-text(`; leaving it out let
/// selector tidying rewrite its argument, padding a regex `+` into ` + `.
pub(crate) const EXTENDED_PSEUDO: [&str; 23] = [
    ":style(",
    ":has-text(",
    ":has(",
    ":remove(",
    ":remove-attr(",
    ":remove-class(",
    ":matches-path(",
    ":matches-css(",
    ":matches-media(",
    ":matches-prop(",
    ":upward(",
    ":xpath(",
    ":watch-attr(",
    ":min-text-length(",
    ":-abp-has(",
    ":-abp-contains(",
    ":contains(",
    ":matches-attr(",
    ":-abp-properties(",
    ":others(",
    // AdGuard's DOM-property match, and uBO's pseudo-element CSS matches:
    // their arguments are regexes, where `+` and `>` are not combinators
    ":matches-property(",
    ":matches-css-before(",
    ":matches-css-after(",
];

/// Sort domains and clean element hiding rules
pub(crate) fn element_tidy(domains: &str, separator: &str, selector: &str) -> String {
    let selector = selector.trim();
    let mut domains = domains.to_ascii_lowercase();

    // Sort domain names alphabetically
    if domains.contains(',') {
        let domain_list: Vec<&str> = domains.split(',').collect();
        let cap = domain_list.len();
        let mut valid_domains: Vec<String> = Vec::with_capacity(cap);
        let mut invalid_domains: Vec<String> = Vec::with_capacity(4);

        for d in &domain_list {
            let stripped = d.trim_start_matches('~');
            // Strip ">>" ancestor-context marker suffix for validation
            let domain_only = stripped.strip_suffix(">>").unwrap_or(stripped);
            let len = domain_only.len();
            let has_dot = domain_only.contains('.');
            // Allow:
            // - * (wildcard for all domains)
            // - TLDs without dots (pl, de, com, org) - must be 2+ chars
            // - Regular domains with dots (example.com) - must be 4+ chars
            let is_valid = domain_only == "*" || (!has_dot && len >= 2) || (has_dot && len >= 4);
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
    let is_extended = match separator {
        "#$#" | "#@$#" | "#%#" | "#@%#" | "#$?#" | "#@$?#" | "$$" | "$@$" => true,
        _ => selector.starts_with("+js(")
            || selector.starts_with("^")
            || selector.starts_with("//scriptlet(")
            || selector.contains(" {")
            // These take literal text, a regex or a declaration as their
            // argument, not a selector, so nothing inside may be tidied.
            // `fop_rules::LITERAL_ARG_CONSTRUCTS` is the same idea for the
            // addition checks. Gated on a `:` so the usual selector, which has
            // none, costs one byte scan rather than 20 substring searches.
            || (selector.contains(':')
                && EXTENDED_PSEUDO.iter().any(|p| selector.contains(p)))
    };

    if is_extended {
        // Normalize scriptlet spacing (only simple args without quotes: uBO
        // accepts ", ' and ` around an argument, and a quoted one may hold a
        // comma).
        // Split only where uBO does: `\,` is a comma inside an argument, so
        // `necessary\,preferences` is one cookie value. Splitting on every
        // comma put a space after the escaped one, which changed the value.
        if selector.starts_with("+js(") && !selector.contains(['"', '\'', '`']) {
            if let Some(start) = selector.find('(') {
                if let Some(end) = selector.rfind(')') {
                    let args = split_unescaped_commas(&selector[start + 1..end])
                        .into_iter()
                        .map(str::trim)
                        .collect::<Vec<_>>()
                        .join(", ");
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

    // Make pseudo classes lowercase.
    //
    // By byte range rather than `replacen`, so a name that occurs more than
    // once is lowercased where it was found instead of at its first occurrence.
    {
        let mut edits: Vec<(usize, usize)> = Vec::new();
        for caps in PSEUDO_PATTERN.captures_iter(&selector) {
            let m = caps.get(1).expect("PSEUDO_PATTERN has one group");
            // Not every `:` introduces a pseudo-class. AdGuard's filters carry
            // both cases: `:contains(/^(?:Reklama$|...)/)`, where lowercasing
            // the group's first alternative changed which text the rule
            // matched, and `###js\:cookies\:barInitWrapper`, where it changed
            // the id being hidden.
            if not_a_pseudo_class(selector.as_bytes(), m.start()) {
                continue;
            }
            let name = m.as_str();
            if selector_only_strings.contains(name)
                || !selector_without_strings.contains(name)
            {
                continue;
            }
            edits.push((m.start(), m.end()));
        }
        // Checked only once a candidate has survived the guards, and only if
        // there is one: it is a regex over the whole selector, and running it
        // on every element rule would cost more than the lowercasing saves.
        if !edits.is_empty() && UNICODE_SELECTOR.is_match(&selector_without_strings) {
            edits.clear();
        }
        // Applied back to front so an earlier edit cannot move a later range.
        for (start, end) in edits.into_iter().rev() {
            let lowered = selector[start..end].to_ascii_lowercase();
            selector.replace_range(start..end, &lowered);
        }
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
    // `len() >= 2` or a lone `/` satisfies both ends and the body slice below
    // panics on it -- reachable from any truncated rule in a list.
    arg.len() >= 2 && arg.starts_with('/') && arg.ends_with('/')
}

/// The flags on a regex argument, or `None` if it is not `/pattern/flags`.
///
/// `rfind` on `arg[1..]` cannot return past `len - 2`, and the byte it finds
/// is an ASCII `/`, so the slice below is always in bounds and on a character
/// boundary.
#[inline]
fn regex_flags(arg: &str) -> Option<&str> {
    if !arg.starts_with('/') || arg.ends_with('/') {
        return None;
    }
    let close = arg[1..].rfind('/')? + 1;
    let flags = &arg[close + 1..];
    // Only flags a JavaScript regex accepts, each at most once. Any trailing
    // letters used to count, so the plain text `/path/to` read as `/path/`
    // with flags `to`; merged, that became `/path|Sponsored/to`, which uBO
    // cannot compile as a regex and so matches as literal text -- nothing.
    let valid = !flags.is_empty()
        && flags.bytes().enumerate().all(|(i, b)| {
            b"dgimsuyv".contains(&b) && !flags.as_bytes()[..i].contains(&b)
        });
    valid.then_some(flags)
}

/// The body of a regex argument, without its slashes or flags.
#[inline]
fn regex_body(arg: &str) -> &str {
    match regex_flags(arg) {
        Some(flags) => &arg[1..arg.len() - flags.len() - 1],
        None => &arg[1..arg.len() - 1],
    }
}

/// Whether an argument is one fop must not fold into a larger regex.
///
/// An empty alternative -- `/foo|/`, or the empty regex `//` -- matches every
/// string, so merging it away silently narrows what the group hides. Flags are
/// handled by the caller instead: a group all carrying the same ones can merge
/// and keep them, while mixing `/foo/i` with plain text would make the plain
/// text case-insensitive too.
#[inline]
fn is_unmergeable_arg(arg: &str) -> bool {
    let regex = is_regex_arg(arg) || regex_flags(arg).is_some();
    // Detected directly rather than by comparing against a naive split, which
    // also differs for a grouped `(a|b)c`.
    regex
        && (regex_body(arg).is_empty()
            || top_level_alternatives(regex_body(arg))
                .iter()
                .any(|a| a.is_empty()))
}

/// Extract the regex content (without slashes) or escape plain text
#[inline]
fn normalize_has_text_arg(arg: &str) -> String {
    if is_regex_arg(arg) || regex_flags(arg).is_some() {
        regex_body(arg).to_string()
    } else {
        escape_regex_chars(arg)
    }
}

/// Bracket balance for text that is not CSS.
///
/// A `:has-text()` argument is literal, so an apostrophe in `Don't miss` is a
/// character rather than an open quote -- the quote-aware check rejects such
/// text and silently declines to merge perfectly good rules.
#[inline]
fn brackets_balance_literal(text: &str) -> bool {
    let (mut round, mut square) = (0i32, 0i32);
    for b in text.bytes() {
        match b {
            b'(' => round += 1,
            b')' => round -= 1,
            b'[' => square += 1,
            b']' => square -= 1,
            _ => {}
        }
        if round < 0 || square < 0 {
            return false;
        }
    }
    round == 0 && square == 0
}

/// Parse a selector to extract base selector and :has-text() argument
pub(crate) fn parse_has_text_selector(selector: &str) -> Option<(String, String, String)> {
    let caps = HAS_TEXT_PATTERN.captures(selector)?;
    let (base, pseudo, arg) = (&caps[1], &caps[2], &caps[3]);
    // The pattern is lazy on the left, so a nested `:has(span:has-text(x))`
    // splits as base `…:has(span` and arg `x)` -- the base loses a paren and
    // the argument gains one. Rebuilding from that yields a rule one `)` short
    // whose regex looks for a literal bracket. Only merge when the split is
    // clean on both sides.
    if !crate::fop_rules::brackets_balance(base) || !brackets_balance_literal(arg) {
        return None;
    }
    Some((base.to_string(), pseudo.to_string(), arg.to_string()))
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

    // Multiple rules - combine into one regex. Alternatives are deduplicated:
    // merging `/A|B/` with `A` and `B`, which is what a part-merged group looks
    // like on the next run, would otherwise grow `/A|B|A|B/` every time.
    // Any argument that cannot be folded leaves the group alone entirely.
    if args.iter().any(|a| is_unmergeable_arg(a)) {
        return String::new();
    }
    // Every argument must already carry the group's flags, with no flags a set
    // of its own. Plain text and an unflagged regex are both case-sensitive, so
    // they fold together losslessly, and `/a/i` with `/b/i` is likewise already
    // agreed -- but `/a/i` with `b` is not. That used to merge as `/a|b/i`, on
    // the reasoning that an author writing `/i` beside it meant `b` too; these
    // are separate rules that happen to share a base selector, though, written
    // at different times and saying nothing about each other. Folding them hid
    // strictly more than the two rules did: uAssets' `:has-text(Sponsored)` on
    // torrentz2 began matching "sponsored" in any case. Five corpora hold two
    // such pairs, and that is the only one the sort actually groups -- a
    // comment separates the other, so its two rules never meet. A cosmetic
    // rule that widens itself on a sort is a false positive nobody asked for.
    //
    // Two different flag sets have no single form to merge into either, so
    // `/a/i` with `/b/m` is left alone.
    let flags = regex_flags(&args[0]).unwrap_or("");
    if args.iter().any(|a| regex_flags(a).unwrap_or("") != flags) {
        return String::new();
    }
    let mut seen: Vec<String> = Vec::with_capacity(args.len());
    for arg in args {
        for alt in top_level_alternatives(&normalize_has_text_arg(arg)) {
            if !seen.iter().any(|s| s == &alt) {
                seen.push(alt);
            }
        }
    }
    format!("/{}/{}", seen.join("|"), flags)
}

/// Split a regex body on its top-level `|`.
///
/// A `|` inside a group or a character class is part of one alternative, so
/// `(a|b)c` stays whole rather than becoming `(a` and `b)c`.
fn top_level_alternatives(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut class, mut escaped, mut start) = (0i32, false, false, 0usize);
    for (i, b) in body.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match b {
            b'\\' => escaped = true,
            b'[' if !class => class = true,
            b']' if class => class = false,
            b'(' if !class => depth += 1,
            b')' if !class => depth -= 1,
            b'|' if !class && depth == 0 => {
                out.push(body[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(body[start..].to_string());
    out
}

/// The cosmetic separator at the start of `rest`, and whether a `:has-text()`
/// group carrying it may be merged.
///
/// A prefix trie on the byte after the `#`, rather than ten strings tried in
/// longest-first order: `##` is far and away the common case and was last in
/// such a list, so every hiding rule paid nine failed comparisons. The
/// ambiguous pairs -- `#@$?#` against `#@$#`, `#$?#` against `#$#` -- are
/// disjoint branches here, so longest match holds by construction rather than
/// by the order of a list.
///
/// Only `##` is mergeable. An exception cancels a hiding rule by matching its
/// selector text, so folding two of them would leave neither original in
/// existence; `#$#`/`#%#` inject CSS and JavaScript, where `:has-text()` means
/// nothing; and `#?#` is left out because merging rewrites
/// `:-abp-contains(text)` into `:-abp-contains(/regex/)`, which assumes the
/// engine reading that separator accepts a regex there.
#[inline]
pub(crate) fn cosmetic_separator(rest: &str) -> Option<(&'static str, bool)> {
    let b = rest.as_bytes();
    match b.get(1)? {
        b'#' => Some(("##", true)),
        b'?' => (b.get(2) == Some(&b'#')).then_some(("#?#", false)),
        b'$' => match b.get(2) {
            Some(b'?') if b.get(3) == Some(&b'#') => Some(("#$?#", false)),
            Some(b'#') => Some(("#$#", false)),
            _ => None,
        },
        b'%' => (b.get(2) == Some(&b'#')).then_some(("#%#", false)),
        b'@' => match b.get(2) {
            Some(b'#') => Some(("#@#", false)),
            Some(b'?') if b.get(3) == Some(&b'#') => Some(("#@?#", false)),
            Some(b'%') if b.get(3) == Some(&b'#') => Some(("#@%#", false)),
            Some(b'$') => match b.get(3) {
                Some(b'#') => Some(("#@$#", false)),
                Some(b'?') if b.get(4) == Some(&b'#') => Some(("#@$?#", false)),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Combine element rules with same domain and base selector but different :has-text() args
pub fn combine_has_text_rules(lines: Vec<String>) -> Vec<String> {
    let capacity = lines.len();
    // (domains, separator, base selector, pseudo-class) -> (position, args)
    type HasTextKey = (String, String, String, String);
    let mut groups: AHashMap<HasTextKey, (usize, Vec<String>)> =
        AHashMap::with_capacity(capacity / 4);
    let mut order: Vec<(usize, String)> = Vec::with_capacity(capacity);
    let mut idx = 0;
    
    for line in lines {
        // Hiding rules only. An exception cancels a hiding rule by matching
        // its selector *text*, so folding `#@#…:has-text(A)` and `…(B)` into
        // `…:has-text(/A|B/)` leaves neither original string in existence and
        // the hiding rules they cancelled are no longer excepted. A hiding
        // rule stands alone, so nothing has to match its text.
        //
        // `#$#`/`#%#` inject CSS and JavaScript, where :has-text() means
        // nothing, and are skipped for that reason instead.
        //
        // Split at the first `#` that begins a separator, not at the first
        // separator appearing anywhere: a selector may carry `#?#` inside an
        // attribute value, and splitting there would group the wrong rules.
        //
        // The scan stops at whatever separator it meets, mergeable or not.
        // Stepping over one it cannot merge would find the `##` formed by that
        // separator's trailing `#` and an ID selector's leading `#` -- reading
        // `a.com#@##ad` as domains `a.com#@`, separator `##`, and merging two
        // exceptions after all.
        let split = (!line.starts_with('!') && !line.starts_with('['))
            .then(|| {
                let mut from = 0;
                while let Some(hash) = line[from..].find('#') {
                    let at = from + hash;
                    if let Some((sep, mergeable)) = cosmetic_separator(&line[at..]) {
                        return mergeable
                            .then(|| (&line[..at], sep, &line[at + sep.len()..]));
                    }
                    from = at + 1;
                }
                None
            })
            .flatten();
        let Some((domains, separator, selector)) = split else {
            order.push((idx, line));
            idx += 1;
            continue;
        };

        if let Some((base, pseudo, arg)) = parse_has_text_selector(selector) {
            let key = (domains.to_string(), separator.to_string(), base, pseudo);
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
    for ((domains, separator, base, pseudo), (pos, args)) in groups {
        // Only track if multiple rules were merged
        let was_merged = args.len() > 1;
               
        let merged_arg = merge_has_text_args(&args);
        // An empty result means the group holds something that must not be
        // folded; put the rules back exactly as they came in.
        if merged_arg.is_empty() {
            // All at `pos`: only the first member of a group advanced `idx`,
            // so `pos + offset` would collide with positions already given to
            // later lines and the stable sort would interleave them. Equal
            // keys keep insertion order, which is the order they arrived in.
            for arg in &args {
                order.push((
                    pos,
                    format!("{}{}{}:{}({})", domains, separator, base, pseudo, arg),
                ));
            }
            continue;
        }
        let merged_rule = format!("{}{}{}:{}({})", domains, separator, base, pseudo, merged_arg);
        
        // Track merge
        if was_merged {
            with_tracked_changes(|changes| {
                let originals: Vec<String> = args
                    .iter()
                    .map(|arg| format!("{}{}{}:{}({})", domains, separator, base, pseudo, arg))
                    .collect();
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

/// Convert extended selectors between syntaxes.
///
/// `abp` rewrites ABP operators to their uBO equivalents (`:-abp-contains(`
/// -> `:has-text(`). It does not touch separators: uBO reads `##` and `#@#`
/// for the rules it produces.
///
/// `adguard` promotes a `:has-text()` rule's separator — `##` -> `#?#` and
/// `#@#` -> `#@?#`. Those spellings are AdGuard's, so they are only right for
/// a list AdGuard consumes, and are a separate switch rather than a side
/// effect of `abp`: a rule can hit them while having nothing for `abp` to
/// convert.
pub(crate) fn convert_selectors(rule: &str, abp: bool, adguard: bool) -> String {
    let mut out = if abp && rule.contains(":-abp-") {
        rule.replace(":-abp-contains(", ":has-text(")
            .replace(":-abp-has(", ":has(")
    } else {
        rule.to_string()
    };

    // Only :has-text() needs the procedural separator; :has() alone is native
    // CSS and works with ##. HTML filtering rules (##^) are uBO-specific — skip.
    if adguard && out.contains(":has-text(") && !out.contains("##^") {
        if out.contains("##") && !out.contains("#?#") {
            out = out.replacen("##", "#?#", 1);
        }
        if out.contains("#@#") && !out.contains("#@?#") {
            out = out.replacen("#@#", "#@?#", 1);
        }
    }
    out
}

/// Combine filters with identical rules but different domains.
///
/// Rules that differ only in their domain list merge, the domains
/// deduplicated and sorted. Merging chains: each rule is tried against the
/// result so far, so a run of mergeable neighbours becomes one line.
///
/// A run is merged in one pass, which is what keeps a group of thousands of
/// domains from being re-parsed and re-sorted at every step. With change
/// tracking on (`--pr-show-changes`) the steps the PR description will list
/// are taken pairwise, so each can be recorded with its intermediate line;
/// once it has all it can show, the rest is merged in one pass and counted.
pub(crate) fn combine_filters(
    uncombined: Vec<String>,
    domain_pattern: &Regex,
    separator: &str,
) -> Vec<String> {
    // No combining needed for single filter
    if uncombined.len() <= 1 {
        return uncombined;
    }
    if !TRACK_CHANGES.load(std::sync::atomic::Ordering::Relaxed) {
        return combine_filters_linear(uncombined, domain_pattern, separator);
    }
    // Recorded locally and handed over once: one lock per call, not per step
    let room = SORT_CHANGES
        .lock()
        .map_or(0, |changes| PR_CHANGES_SHOWN.saturating_sub(changes.domains_combined.len()));
    let mut record = CombineRecord { steps: Vec::new(), room, count: 0 };
    let combined = combine_filters_recorded(uncombined, domain_pattern, separator, &mut record);
    if record.count > 0 {
        with_tracked_changes(|changes| {
            // Other files may have filled it since
            let room = PR_CHANGES_SHOWN.saturating_sub(changes.domains_combined.len());
            changes.domains_combined.extend(record.steps.into_iter().take(room));
            changes.domains_combined_count += record.count;
        });
    }
    combined
}

/// The merge steps one combine_filters call records.
pub(crate) struct CombineRecord {
    /// (the two rules, the merged line), at most `room` of them
    pub steps: Vec<(Vec<String>, String)>,
    pub room: usize,
    /// Every merge step, recorded or not
    pub count: usize,
}

/// Merge as combine_filters does, recording steps in full while `record`
/// has room and only counting them after.
pub(crate) fn combine_filters_recorded(
    mut uncombined: Vec<String>,
    domain_pattern: &Regex,
    separator: &str,
    record: &mut CombineRecord,
) -> Vec<String> {
    let mut combined: Vec<String> = Vec::with_capacity(uncombined.len());
    for i in 0..uncombined.len() {
        if record.steps.len() >= record.room {
            // Nothing more will be listed: merge the rest in one pass. A chain
            // takes one step per rule merged away, so that is the count.
            let rest: Vec<String> = uncombined.drain(i..).collect();
            let rest_len = rest.len();
            let merged = combine_filters_linear(rest, domain_pattern, separator);
            record.count += rest_len - merged.len();
            combined.extend(merged);
            break;
        }
        if i + 1 < uncombined.len() {
            if let Some(merged) = combine_pair(&uncombined[i], &uncombined[i + 1], domain_pattern, separator) {
                record.count += 1;
                record.steps.push((vec![uncombined[i].clone(), uncombined[i + 1].clone()], merged.clone()));
                // The merged rule is tried against the next one in turn
                uncombined[i + 1] = merged;
                continue;
            }
        }
        combined.push(std::mem::take(&mut uncombined[i]));
    }
    combined
}

/// Order of a merged domain list: by name, an exclusion after its inclusion.
#[inline]
fn cmp_domains(a: &str, b: &str) -> Ordering {
    let (a_base, a_inv) = a.strip_prefix('~').map_or((a, false), |s| (s, true));
    let (b_base, b_inv) = b.strip_prefix('~').map_or((b, false), |s| (s, true));
    (a_base, a_inv).cmp(&(b_base, b_inv))
}

/// Merge `second` into `first` if they differ only in their domains.
/// One step of the chain, and the definition the linear path reproduces.
fn combine_pair(first: &str, second: &str, domain_pattern: &Regex, separator: &str) -> Option<String> {
    let domains1 = domain_pattern.captures(first)?;
    let domains2 = domain_pattern.captures(second)?;
    let domain1_str = domains1.get(1).map_or("", |m| m.as_str());
    let domains1_full = domains1.get(0).map_or("", |m| m.as_str());
    if domain1_str.is_empty() {
        return None;
    }
    let domain2_str = domains2.get(1).map_or("", |m| m.as_str());
    if domain2_str.is_empty() {
        return None;
    }
    let domains2_full = domains2.get(0).map_or("", |m| m.as_str());

    // Check if domain patterns are compatible (same structure except domain list)
    if domains1_full.replace(domain1_str, domain2_str) != domains2_full {
        return None;
    }

    // Check if filters are identical except for domains
    if domain_pattern.replace(first, "") != domain_pattern.replace(second, "") {
        return None;
    }

    // Check for mixed include/exclude domains
    let domain1_only_excludes = domain1_str.matches('~').count() == domain1_str.split(separator).count();
    let domain2_only_excludes = domain2_str.matches('~').count() == domain2_str.split(separator).count();
    if domain1_only_excludes != domain2_only_excludes {
        return None;
    }

    // Combine domains
    let mut new_domains: Vec<&str> = domain1_str
        .split(separator)
        .chain(domain2_str.split(separator))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    new_domains.sort_unstable_by(|a, b| cmp_domains(a, b));
    let new_domain_str = new_domains.join(separator);

    // Create the substitution pattern (full match with new domains)
    let domains_substitute = domains1_full.replace(domain1_str, &new_domain_str);

    // Escape $ for regex replacement ($ is special in replacement strings)
    let escaped_substitute = if domains_substitute.contains('$') {
        domains_substitute.replace("$", "$$")
    } else {
        domains_substitute
    };

    Some(domain_pattern.replace(first, escaped_substitute.as_str()).into_owned())
}

/// Bytes a domain must not hold for the linear path to take it. With none of
/// them present the domain list is found at the same place however it
/// grows, and occurs only once in the match, which is what lets the linear
/// path skip re-parsing: `,` and `|` separate, `#`, `@`, `?`, `$` and `%`
/// open a cosmetic separator, and `=` ends `domain=`.
const UNSAFE_DOMAIN_BYTES: &[u8] = b",|#@?$%=";

/// A rule taking part in a linear merge: the rule whose shape the merged line
/// keeps, where its domain list sits, and the domains gathered so far.
struct DomainRun<'a> {
    idx: usize,
    full: std::ops::Range<usize>,
    dom: std::ops::Range<usize>,
    /// None until something merges in; then the deduplicated domains
    domains: Option<HashSet<&'a str>>,
    /// Byte length of the domains in `domains`, separators excluded
    domain_bytes: usize,
    /// `~` count and entry count of the domain list as it stands, which is
    /// how combine_pair tells an exclusion-only list
    tildes: usize,
    total: usize,
}

impl<'a> DomainRun<'a> {
    /// Parse rule `idx`, or None when combine_pair's replacements could act
    /// on more than the domain list, so the step-by-step path must decide.
    fn parse(idx: usize, rules: &'a [String], domain_pattern: &Regex, separator: &str) -> Option<Self> {
        let line = rules[idx].as_str();
        let caps = domain_pattern.captures(line)?;
        let full = caps.get(0)?.range();
        let dom = caps.get(1)?.range();
        let before = &line.as_bytes()[full.start..dom.start];
        let after = &line.as_bytes()[dom.end..full.end];
        // A domain list growing across what surrounds it could otherwise be
        // found at a second place in the match
        if before.last().is_some_and(|b| !UNSAFE_DOMAIN_BYTES.contains(b))
            || !after.iter().all(|b| UNSAFE_DOMAIN_BYTES.contains(b))
        {
            return None;
        }
        let before = &line[full.start..dom.start];
        let (mut tildes, mut total) = (0, 0);
        for domain in line[dom.clone()].split(separator) {
            if domain.is_empty()
                || domain.bytes().any(|b| UNSAFE_DOMAIN_BYTES.contains(&b))
                || (!before.is_empty() && before.contains(domain))
            {
                return None;
            }
            tildes += domain.bytes().filter(|&b| b == b'~').count();
            total += 1;
        }
        // A merged list inside `before` would hold each of its domains there
        // too, so checking them one by one covers every list they can form
        Some(DomainRun { idx, full, dom, domains: None, domain_bytes: 0, tildes, total })
    }

    /// Merge `next` in if combine_pair would, without building the line.
    /// None when only combine_pair can tell.
    fn absorb(&mut self, next: &DomainRun<'a>, rules: &'a [String], separator: &str) -> Option<bool> {
        let line = rules[self.idx].as_bytes();
        let next_line = rules[next.idx].as_bytes();

        let (before, after) = (&line[self.full.start..self.dom.start], &line[self.dom.end..self.full.end]);
        // next's domains were vetted against its own lead-in; merged they sit
        // behind ours. Split at a different point, only combine_pair can say
        // whether the two matches agree.
        if next.dom.start - next.full.start != before.len() {
            return None;
        }
        // Same match with next's domains in place of ours: with the lead-ins
        // the same length, the same lead-in and tail
        let next_full = &next_line[next.full.clone()];
        if next_full.len() != before.len() + next.dom.len() + after.len()
            || !next_full.starts_with(before)
            || !next_full.ends_with(after)
        {
            return Some(false);
        }

        // Same rule with the match removed
        let (head, tail) = (&line[..self.full.start], &line[self.full.end..]);
        let (next_head, next_tail) = (&next_line[..next.full.start], &next_line[next.full.end..]);
        if head.len() + tail.len() != next_head.len() + next_tail.len()
            || !head.iter().chain(tail).eq(next_head.iter().chain(next_tail))
        {
            return Some(false);
        }

        if (self.tildes == self.total) != (next.tildes == next.total) {
            return Some(false);
        }

        let domains = match self.domains.as_mut() {
            Some(domains) => domains,
            None => {
                let own = &rules[self.idx][self.dom.clone()];
                let mut domains = HashSet::with_capacity(self.total + next.total);
                let (mut tildes, mut bytes) = (0, 0);
                for domain in own.split(separator) {
                    if domains.insert(domain) {
                        tildes += domain.bytes().filter(|&b| b == b'~').count();
                        bytes += domain.len();
                    }
                }
                self.tildes = tildes;
                self.domain_bytes = bytes;
                self.domains.insert(domains)
            }
        };
        for domain in rules[next.idx][next.dom.clone()].split(separator) {
            if domains.insert(domain) {
                self.tildes += domain.bytes().filter(|&b| b == b'~').count();
                self.domain_bytes += domain.len();
            }
        }
        self.total = domains.len();
        Some(true)
    }

    /// The finished line: the original rule, or the merged one.
    fn finish(self, rules: &[String], domain_pattern: &Regex, separator: &str) -> MergedLine {
        let Some(domains) = self.domains else {
            return MergedLine::Original(self.idx);
        };
        let line = rules[self.idx].as_str();
        let mut domains: Vec<&str> = domains.into_iter().collect();
        domains.sort_unstable_by(|a, b| cmp_domains(a, b));
        let list_len = self.domain_bytes + separator.len() * (domains.len() - 1);
        let mut merged = String::with_capacity(line.len() - self.dom.len() + list_len);
        merged.push_str(&line[..self.dom.start]);
        for (i, domain) in domains.iter().enumerate() {
            if i > 0 {
                merged.push_str(separator);
            }
            merged.push_str(domain);
        }
        merged.push_str(&line[self.dom.end..]);
        debug_assert_eq!(
            domain_pattern.captures(&merged).and_then(|c| c.get(1)).map(|m| m.range()),
            Some(self.dom.start..self.dom.start + list_len),
            "merged domain list moved: {}", merged
        );
        MergedLine::New(merged)
    }
}

/// An output line of the linear merge: an input rule untouched, or a new one.
enum MergedLine {
    Original(usize),
    New(String),
}

impl MergedLine {
    fn as_str<'b>(&'b self, rules: &'b [String]) -> &'b str {
        match self {
            MergedLine::Original(idx) => &rules[*idx],
            MergedLine::New(line) => line,
        }
    }
}

/// Where the linear merge stands: a run it can extend itself, or a line
/// only combine_pair can judge.
enum MergeState<'a> {
    Run(DomainRun<'a>),
    Line(MergedLine),
}

/// Merge as chaining combine_pair does, producing the same lines, but in
/// one pass: a run of mergeable rules collects its domains in a set, sorted
/// and joined once at the end, instead of every step re-parsing, re-sorting
/// and rebuilding the line so far. A rule whose domains could make the
/// shortcut differ takes the pairwise step instead (DomainRun::parse).
pub(crate) fn combine_filters_linear(
    mut uncombined: Vec<String>,
    domain_pattern: &Regex,
    separator: &str,
) -> Vec<String> {
    let lines = {
        let rules = uncombined.as_slice();
        let state_of = |idx: usize| match DomainRun::parse(idx, rules, domain_pattern, separator) {
            Some(run) => MergeState::Run(run),
            None => MergeState::Line(MergedLine::Original(idx)),
        };
        let finish = |state: MergeState| match state {
            MergeState::Run(run) => run.finish(rules, domain_pattern, separator),
            MergeState::Line(line) => line,
        };
        let mut lines: Vec<MergedLine> = Vec::with_capacity(rules.len());
        let mut state = state_of(0);
        for idx in 1..rules.len() {
            let next = state_of(idx);
            // Extend the run where DomainRun can decide; otherwise settle the
            // line so far and let combine_pair judge the step
            let (current, next) = match (state, next) {
                (MergeState::Run(mut run), MergeState::Run(next)) => match run.absorb(&next, rules, separator) {
                    Some(true) => {
                        state = MergeState::Run(run);
                        continue;
                    }
                    Some(false) => {
                        lines.push(run.finish(rules, domain_pattern, separator));
                        state = MergeState::Run(next);
                        continue;
                    }
                    None => (MergeState::Run(run), MergeState::Run(next)),
                },
                pair => pair,
            };
            let current = finish(current);
            state = match combine_pair(current.as_str(rules), &rules[idx], domain_pattern, separator) {
                Some(merged) => MergeState::Line(MergedLine::New(merged)),
                None => {
                    lines.push(current);
                    next
                }
            };
        }
        lines.push(finish(state));
        lines
    };
    lines
        .into_iter()
        .map(|line| match line {
            MergedLine::Original(idx) => std::mem::take(&mut uncombined[idx]),
            MergedLine::New(line) => line,
        })
        .collect()
}

// =============================================================================
// Main Sorting Function
// =============================================================================

/// The host at the start of a rule, and where it ends: `DOMAIN_EXTRACT_PATTERN`
/// (`^\|*([^/\^\$]+)`) as a byte scan. Skip the leading `|`, then take up to
/// the first `/`, `^` or `$`.
///
/// The two agree on all but one line of 2.6M across four corpora:
/// `|/nbsys3/fsyspp.js`, where the regex backtracks -- `\|*` gives up its `|`
/// so the group can take it -- and calls the host `|`. The scan says there is
/// no host, which is the truer answer, and nothing downstream can tell: a
/// host of `|` is followed by a path, so neither warns.
///
/// 70% of network rules reach this -- 860k of the 1.23M in a 1.67M-line corpus
/// -- and `captures` builds and fills a capture group for every one, to hand
/// back a slice a scan finds directly. Worth 8% of the whole sort.
#[inline]
pub(crate) fn extract_leading_host(line: &str) -> Option<(&str, usize)> {
    let bytes = line.as_bytes();
    let start = bytes.iter().take_while(|&&b| b == b'|').count();
    let end = start
        + bytes[start..]
            .iter()
            .take_while(|&&b| b != b'/' && b != b'^' && b != b'$')
            .count();
    (end > start).then(|| (&line[start..end], end))
}

/// Whether a line could carry a cosmetic separator at all.
///
/// Every separator the element patterns accept holds a `#` -- `##`, `#@#`,
/// `#?#`, `#$#`, `#%#` and the AdGuard extended forms -- bar AdGuard's HTML
/// filters, `$$` and `$@$`. A line with neither character cannot match, and
/// the patterns that decide are the expensive kind: a lazy `([^/|@"!]*?)` and
/// three capture groups, which puts the regex crate on its backtracking and
/// PikeVM engines rather than a DFA. They were the two hottest functions in a
/// profile of a real sort, above anything in FOP itself.
///
/// 78% of rules in a 1.58M-line corpus hold no `#`, so most lines now settle
/// this with one `memchr` pass instead.
#[inline]
fn may_be_element_rule(line: &str, parse_adguard: bool) -> bool {
    line.contains('#') || (parse_adguard && line.contains('$'))
}

/// Whether a line is a plain-text comment: `#` alone, or `#` then whitespace.
///
/// Hosts files and the plain URL registries that ship beside filter lists --
/// uAssets' `badlists.txt` among them -- comment with `#`, while fop's comment
/// character defaults to `!`. Such a line matches no cosmetic separator either,
/// since every one of those is at least two characters (`##`, `#@#`, `#?#`,
/// `#$#`, `#%#`), so it used to fall through to the network-rule path: its
/// whitespace was stripped (`# two words` became `#twowords`) and it sorted as
/// a rule, away from the lines it introduced.
///
/// Requiring the whitespace keeps this narrow, and it must be followed by
/// something: `#foo` stays a rule, because only `# foo` is the documented
/// hosts convention, and a lone `#` introduces nothing, so it is left to the
/// line-length minimum that removes any other one-character line.
#[inline]
pub(crate) fn is_plain_comment(line: &str) -> bool {
    match line.as_bytes() {
        [b'#', rest @ ..] => rest.first().is_some_and(u8::is_ascii_whitespace),
        _ => false,
    }
}

/// One rule as the sort writes it, without merging it with any other.
///
/// The addition checks run before sorting, so that they judge -- and
/// `--remove-bad-rules` deletes -- only lines the author wrote. Run after, they
/// saw the sort's merges: an added `b..com##.ad` beside a committed
/// `a.com##.ad` became `a.com,b..com##.ad`, which was flagged and deleted,
/// taking the committed rule with it. But the sort also repairs rules as it
/// goes (`$third-party.script` becomes `$script,third-party`, `redirect_rule`
/// becomes `redirect-rule`), so a line is judged in the form it will be
/// written: deleting a rule fop would have repaired is not removing a bad one.
///
/// Mirrors the per-line pipeline in `fop_sort` -- comments and AdGuard rule
/// modifiers pass through, regex-domain rules go through `filter_tidy`, element
/// rules through `element_tidy` and the selector conversions, network rules
/// through `filter_tidy`, and both through the typo fixes when `fix_typos` is
/// on -- and leaves out only what depends on the rest of the file: merging and
/// ordering. `config` must be the one this file is sorted with, per-file
/// overrides included. A test runs both over the same rules under several
/// configs, so the two cannot drift apart unnoticed.
///
/// Silent: the sort raises any warning again when it writes the line.
pub(crate) fn tidy_rule<'a>(line: &'a str, config: &SortConfig) -> Cow<'a, str> {
    let _silent = crate::SuppressWarnings::new();
    let line = line.trim();
    let is_comment = config.comment_chars.iter().any(|c| line.starts_with(c.as_str()))
        || is_plain_comment(line)
        || line.starts_with("%include")
        || (line.starts_with('[') && line.ends_with(']'));
    // Hosts entries are not filter rules, and `[$...]` modifiers pass through
    // the sort untouched.
    // `is_localhost_entry` rather than `config.localhost`: the sorter keeps a
    // hosts entry as written in every mode, and this has to judge the line the
    // sorter will write, not the one a filter-list tidy would make of it.
    if line.is_empty()
        || is_comment
        || config.localhost
        || is_localhost_entry(line)
        || line.starts_with("[$")
    {
        return Cow::Borrowed(line);
    }
    if line.starts_with('/') && REGEX_ELEMENT_PATTERN.is_match(line) {
        return Cow::Owned(filter_tidy(line, config.convert_ubo));
    }
    let element_caps = if !may_be_element_rule(line, config.parse_adguard) {
        None
    } else if config.alt_sort {
        ELEMENT_PATTERN.captures(line)
    } else if config.parse_adguard {
        ADGUARD_ELEMENT_PATTERN.captures(line)
    } else {
        FOPPY_ELEMENT_PATTERN.captures(line)
    };
    let mut tidied = match element_caps {
        Some(caps) => {
            let mut tidied = element_tidy(&caps[1].to_ascii_lowercase(), &caps[2], &caps[3]);
            if config.abp_convert || config.adguard_convert {
                tidied = convert_selectors(&tidied, config.abp_convert, config.adguard_convert);
            }
            if config.convert_trusted {
                if let Some(converted) = convert_trusted_scriptlet(&tidied) {
                    tidied = converted;
                }
            }
            tidied
        }
        None => filter_tidy(line, config.convert_ubo),
    };
    if config.fix_typos {
        let (fixed, fixes) = fop_typos::fix_all_typos(&tidied);
        if !fixes.is_empty() {
            tidied = fixed;
        }
    }
    Cow::Owned(tidied)
}

/// Create `path` afresh for writing, never through a symlink.
///
/// Every file FOP creates beside a list -- the sort's temp file, `.backup`,
/// `--changed`, `.diff`, warnings -- has a predictable name, so a repository
/// could plant a symlink there aimed at a file elsewhere, and a plain create
/// would write through it. A symlink at the path is refused and a stale regular
/// file replaced; `create_new` makes the final check and the creation one step,
/// so a link that appears in between fails the open rather than being followed.
pub(crate) fn create_file_no_follow(path: &Path) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is a symlink; refusing to write through it", path.display()),
            ));
        }
        Ok(meta) if meta.is_dir() => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} is a directory", path.display())));
        }
        Ok(_) => fs::remove_file(path)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    fs::OpenOptions::new().write(true).create_new(true).open(path)
}

/// `fs::write`, through `create_file_no_follow`.
pub(crate) fn write_file_no_follow(path: &Path, contents: &[u8]) -> io::Result<()> {
    create_file_no_follow(path)?.write_all(contents)
}

/// Why `line` can never be a valid filter rule, or `None` if it might be.
///
/// Deliberately narrow. The set of characters that can *begin* a valid rule is
/// open-ended — an allowlist of them deleted real rules starting with `_ % ^ =`
/// and had to be reverted (94266d6) — but the set that can never begin one is
/// closed, so match that instead. Catches only debris that leads with it:
/// broader malformed-rule detection needs a parser, not a character check.
#[inline]
pub fn malformed_rule_reason(line: &str, ignore_minimum: bool) -> Option<&'static str> {
    match line.as_bytes().first() {
        // Debris from a truncated selector, e.g. the tail of `[href="x"])`.
        Some(b'"' | b')' | b']' | b'}') => Some("invalid start"),
        // An author who deliberately wrote a one- or two-character rule keeps
        // it under `ignore_minimum`; the debris check above still applies.
        _ if ignore_minimum => None,
        // `##a` (hide every <a>), `*/*` and `/a/` are all valid three-character
        // rules, so the floor is 3 — not 4, which deleted them.
        //
        // Counted in characters, not bytes: a lone multi-byte character (or a
        // stray UTF-8 BOM, which is 3 bytes) is not a rule, and a byte floor
        // would keep it. `take(3)` so a long line stops after three.
        _ if line.chars().take(3).count() < 3 => Some("too short"),
        _ => None,
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
    let Ok(original_content) = fs::read(filename) else {
        eprintln!("Cannot open {}", filename.display());
        return Ok(None);
    };
    // Detect Windows line endings
    if original_content.windows(2).any(|w| w == b"\r\n") {
        crate::CRLF_FILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // `--localhost` for a file that plainly is one. The bytes are already
    // read, so this costs no extra I/O; see looks_like_hosts_file.
    let localhost = config.localhost || looks_like_hosts_file(&original_content);

    let reader = BufReader::new(Cursor::new(&original_content));
    let mut output = match create_file_no_follow(&temp_file) {
        Ok(f) => BufWriter::with_capacity(64 * 1024, f),
        Err(e) => {
            eprintln!("Cannot create temp file for {}: {}", filename.display(), e);
            return Ok(None);
        }
    };

    let mut section: Vec<String> = Vec::with_capacity(2000);
    let mut lines_checked: usize = 1;
    // Set by an AdGuard hint (`!+ PLATFORM(...)`, `!+ NOT_OPTIMIZED`), which
    // applies to the line after it: that rule must stay where it is.
    let mut hint_pending = false;
    let mut filter_lines: usize = 0;
    let mut element_lines: usize = 0;

    let write_filters = |section: &mut Vec<String>,
                         output: &mut BufWriter<File>,
                         element_lines: usize,
                         filter_lines: usize,
                         no_sort: bool,
                         alt_sort: bool,
                         localhost: bool,
                         parse_adguard: bool|
     -> io::Result<()> {
        if section.is_empty() {
            return Ok(());
        }

        // Collect duplicates locally, merge once (reduces lock contention)
        let track_changes = TRACK_CHANGES.load(std::sync::atomic::Ordering::Relaxed);
        let mut dupes_local: HashSet<String> = HashSet::new();

        // Remove duplicates while preserving order if no_sort
        let mut unique: Vec<String> = {
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
                unique.sort_by_cached_key(|s| localhost_domain(s).to_ascii_lowercase());
            }
            for filter in unique {
                write!(output, "{}\n", filter)?;
            }
        } else if element_lines > filter_lines {
            if !no_sort {
                let pattern = if parse_adguard {
                    &*ADGUARD_ELEMENT_DOMAIN_PATTERN
                } else if alt_sort {
                    &*ELEMENT_DOMAIN_PATTERN
                } else {
                    &*FOPPY_ELEMENT_DOMAIN_PATTERN
                };
                unique.sort_by_cached_key(|s| pattern.replace(s, "").into_owned());
            }
            // Merge :has-text() rules first, then combine domains
            let merged = combine_has_text_rules(unique);
            let combine_pattern = if parse_adguard {
                &*ADGUARD_ELEMENT_DOMAIN_PATTERN
            } else {
                &*ELEMENT_DOMAIN_PATTERN
            };
            let combined = combine_filters(merged, combine_pattern, ",");
            for filter in combined {
                write!(output, "{}\n", filter)?;
            }
        } else {
            // Sort blocking rules (unless no_sort)
            if !no_sort {
                unique.sort_by(|a, b| cmp_ascii_case_insensitive(a, b));
            }
            let combined = combine_filters(unique, &FILTER_DOMAIN_PATTERN, "|");
            for filter in combined {
                write!(output, "{}\n", filter)?;
            }
        }

        Ok(())
    };

    for line in reader.lines() {
        let line_owned = line?;
        let line = line_owned.trim();

        // The rule an AdGuard hint applies to is a section of its own: it goes
        // through every step any rule does, but sorting could move another
        // rule under the hint, and merging would widen it to other rules'
        // domains. The hint closed the section above it, so once that rule is
        // in, close this one. Should the rule be dropped instead, the hint
        // stays with whichever rule now follows it.
        if hint_pending && !section.is_empty() {
            write_filters(
                &mut section,
                &mut output,
                element_lines,
                filter_lines,
                config.no_sort,
                config.alt_sort,
                localhost,
                config.parse_adguard,
            )?;
            lines_checked = 1;
            filter_lines = 0;
            element_lines = 0;
            hint_pending = false;
        }

        // Update timestamp if enabled and within first 10 lines
        let updated_line;
        let line = if config.add_timestamp && lines_checked <= CHECK_LINES {
            if let Some(updated) = update_timestamp_line(line) {
                updated_line = updated;
                updated_line.as_str()
            } else if let Some(updated) = update_version_line(line) {
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
                        localhost,
                        config.parse_adguard,
                    )?;
                    lines_checked = 1;
                    filter_lines = 0;
                    element_lines = 0;
                }
                output.write_all(b"\n")?;
            }
            continue;
        }

        // Comments and special lines
        let is_comment = config.comment_chars.iter().any(|c| line.starts_with(c))
            || is_plain_comment(line)
            || (localhost
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
                    localhost,
                    config.parse_adguard,
                )?;
                lines_checked = 1;
                filter_lines = 0;
                element_lines = 0;
            }
            write!(output, "{}\n", line)?;
            // A chain of hints still targets the first rule after it; any
            // other comment ends it
            hint_pending = line.starts_with("!+");
            continue;
        }

        // A hosts entry is `IP<space>host`, and the space is the syntax --
        // `filter_tidy` strips whitespace from anything that is not an element
        // rule, which turns `0.0.0.0 keep.com` into `0.0.0.0keep.com`. That
        // held in every mode, so a hosts file sorted without `--localhost`
        // came out with every entry run together, and silently: nothing
        // downstream reads a mangled entry as an error. There is nothing in
        // such a line for the tidier to do, so it is kept as written whether
        // or not the file was recognised as a hosts file.
        //
        // Ahead of every other check, as the `--localhost` block it replaces
        // was: an entry is not a filter rule, so no rule check has anything to
        // say about it.
        if is_localhost_entry(line) {
            section.push(line.to_string());
            continue;
        }

        // Dropping what is left is for an explicit --localhost only. That flag
        // is the caller stating the file is a hosts file, so a line that is
        // not an entry is a mistake in it. Detection is a guess, and a guess
        // must not delete a rule: in a detected file the line is sorted as the
        // filter rule it appears to be.
        if config.localhost {
            write_warning(&format!("Removed invalid localhost entry: {}", line));
            continue;
        }

        if let Some(reason) = malformed_rule_reason(line, config.ignore_line_minimum) {
            write_warning(&format!("Removed malformed rule ({}): {}", reason, line));
            continue;
        }

        // [$path=/\/(dom|pro)/]rambler.ru##div[style^="order:"][style*="-1"]
        // AdGuard cosmetic rule modifiers - pass through unchanged
        if line.starts_with("[$") {
            section.push(line.to_string());
            continue;
        }

        // Handle regex domain rules (uBO) - pass through unchanged
        if line.starts_with('/') && REGEX_ELEMENT_PATTERN.is_match(line) {
            section.push(filter_tidy(line, config.convert_ubo));
            continue;
        }

        // Process element hiding rules
        let element_caps = if !may_be_element_rule(line, config.parse_adguard) {
            None
        } else if config.alt_sort {
            ELEMENT_PATTERN.captures(line)
        } else if config.parse_adguard {
            ADGUARD_ELEMENT_PATTERN.captures(line)
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

            // Convert extended selectors between syntaxes
            if config.abp_convert || config.adguard_convert {
                let original = tidied.clone();
                tidied = convert_selectors(&tidied, config.abp_convert, config.adguard_convert);

                if tidied != original && !config.quiet {
                    write_warning(&format!(
                        "Converted selector: {}",
                        tidied
                    ));
                }
            }

            // Convert trusted scriptlets to non-trusted when value is safe
            if config.convert_trusted {
                if let Some(converted) = convert_trusted_scriptlet(&tidied) {
                    if !config.quiet {
                        write_warning(&format!(
                            "Converted trusted scriptlet: {}",
                            converted
                        ));
                    }
                    tidied = converted;
                }
            }

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

        // A network rule whose domain carries no dot.
        //
        // Deleting these was wrong: `||cfd^$third-party,popup,domain=multiup.io`
        // blocks an abuse TLD outright, `||countly-` matches a host prefix and
        // `||com/services/?rt=` a path under any .com host. 11 such rules in
        // AdguardFilters and 11 in uAssets were dropped, reported by a warning
        // and gone from the list. A typo like `||exmaple^` looks exactly the
        // same to FOP, so the rule is kept and mentioned instead --
        // `--ignore-dot-domains` silences the mention -- and the addition
        // checks, which run with the author present, are where a new one is
        // judged.
        //
        // Only where a typo could hide, which is a pattern that is nothing but
        // the host. A rule going on to name a path or a wildcard
        // (`||com/*/ModalEngage|`), one matching a host prefix
        // (`||chamsocthe-`), and one leaving the host to `ipaddress=`
        // (`||cc^$doc,ipaddress=15.207.81.128`) were all built deliberately:
        // nobody mistypes a domain and then writes a path under it. Warning on
        // them buried the rest -- 109 mentions on uAssets, 278 over four
        // corpora, not one of them a typo -- so the shapes that cannot be one
        // stay quiet, leaving 25.
        if (line.starts_with("||") || line.starts_with('|'))
            && !SKIP_SCHEMES.iter().any(|s| line.starts_with(s))
        {
            if let Some((domain, at)) = extract_leading_host(line) {
                // Ordered so the one test that rejects nearly every rule comes
                // first: a domain holding a dot is the overwhelming case, and
                // everything after it then runs on the few that do not. The
                // IPv4 check that used to run here -- a regex, on every rule
                // beginning `|` -- cannot match past it at all, since
                // `^\d+\.\d+\.\d+\.\d+` needs three dots and this domain has
                // none; only the IPv6 `[` remains. The slice below is last
                // because it is the only test that looks beyond the domain.
                if !config.ignore_dot_domains
                    && !domain.contains('.')
                    && !domain.starts_with('[')
                    && !domain.contains('*')
                    && !domain.starts_with('~')
                    && !domain.ends_with('-')
                    && !line.contains("ipaddress=")
                    && {
                        // What follows the host, past the anchors that end it.
                        let tail = line[at..].trim_start_matches(['^', '|']);
                        tail.is_empty() || tail.starts_with('$')
                    }
                {
                    write_warning(&format!(
                        "Kept a network rule with no dot in its domain: {} (domain: {}) -- \
                         a whole-TLD or prefix match. Check it is not a typo.",
                        line, domain
                    ));
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
            localhost,
            config.parse_adguard,
        )?;
    }

    drop(output);

    // A benchmark times the sort. Nothing reads the result, and comparing it
    // or building a diff -- which on a heavily reordered list takes far longer
    // than the sort -- is not what is being measured.
    if config.benchmark {
        fs::remove_file(&temp_file)?;
        return Ok(None);
    }

    // Compare files and replace if different
    let new_content = fs::read(&temp_file)?;

    if original_content != new_content {
        if config.dry_run {
            if config.output_changed {
                // Write to filename--changed.ext
                let stem = filename.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
                let ext = filename.extension().and_then(|e| e.to_str()).unwrap_or("txt");
                let changed_filename = filename.with_file_name(format!("{}--changed.{}", stem, ext));
                
                write_file_no_follow(&changed_filename, &new_content)?;
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
                write_file_no_follow(&backup_file, &original_content)?;
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
