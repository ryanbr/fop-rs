//! Datestamp/timestamp support for filter list headers
//!
//! Handles `! Last modified:` and `! Version:` lines in filter lists.

use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use owo_colors::OwoColorize;

// =============================================================================
// Timestamp Detection
// =============================================================================

/// Check if line is a timestamp line (Last modified/Last updated)
#[inline]
pub fn is_timestamp_line(line: &str) -> bool {
    let line = line.as_bytes();
    // Look for "last modified:" or "last updated:" case-insensitively
    line.windows(14).any(|w| w.eq_ignore_ascii_case(b"last modified:"))
        || line.windows(13).any(|w| w.eq_ignore_ascii_case(b"last updated:"))
}

/// Check if line is a version line
#[inline]
pub fn is_version_line(line: &str) -> bool {
    let trimmed = line.trim_start().trim_start_matches(['!', '#']).trim_start();
    trimmed.len() >= 8 && trimmed[..8].eq_ignore_ascii_case("version:")
}

// =============================================================================
// Timestamp Formatting
// =============================================================================

/// Format Unix timestamp as "30 Jan 2026 08:31 UTC"
#[inline]
pub fn format_timestamp_utc(secs: u64) -> String {
    const MONTHS: [&str; 12] = ["Jan","Feb","Mar","Apr","May","Jun",
                                 "Jul","Aug","Sep","Oct","Nov","Dec"];
    let mut result = String::with_capacity(24);
    let (year, month, day, hours, minutes) = decompose_utc(secs);
    use std::fmt::Write;
    let _ = write!(result, "{} {} {} {:02}:{:02} UTC", day, MONTHS[month], year, hours, minutes);
    result
}

/// Format Unix timestamp as version "YYYYMMDDHHMM"
#[inline]
pub fn format_version_utc(secs: u64) -> String {
    let (year, month, day, hours, minutes) = decompose_utc(secs);
    format!("{}{:02}{:02}{:02}{:02}", year, month + 1, day, hours, minutes)
}

/// Decompose Unix timestamp into (year, month_0indexed, day, hours, minutes)
fn decompose_utc(secs: u64) -> (u64, usize, u64, u64, u64) {
    const DAYS_IN_MONTH: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let mut year = 1970u64;
    loop {
        let diy = if is_leap_year(year) { 366 } else { 365 };
        if days < diy { break; }
        days -= diy;
        year += 1;
    }
    let leap = is_leap_year(year);
    let mut month = 0usize;
    for (i, &d) in DAYS_IN_MONTH.iter().enumerate() {
        let dim = if i == 1 && leap { 29 } else { d };
        if days < dim { month = i; break; }
        days -= dim;
    }
    (year, month, days + 1, hours, minutes)
}

#[inline]
fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

// =============================================================================
// Line Update Functions (for use during sorting)
// =============================================================================

/// Update timestamp in header line (returns updated line or None if not a timestamp line)
#[inline]
pub fn update_timestamp_line(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("last modified:") && !lower.contains("last updated:") {
        return None;
    }
    let prefix = if line.trim_start().starts_with('#') { "#" } else { "!" };
    let keyword = if lower.contains("last modified:") { "Last modified" } else { "Last updated" };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    Some(format!("{} {}: {}", prefix, keyword, format_timestamp_utc(now)))
}

/// Update version line in header (returns updated line or None if not a version line)
#[inline]
pub fn update_version_line(line: &str) -> Option<String> {
    if !is_version_line(line) {
        return None;
    }
    let prefix = if line.trim_start().starts_with('#') { "#" } else { "!" };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    Some(format!("{} Version: {}", prefix, format_version_utc(now)))
}

// =============================================================================
// File Operations
// =============================================================================

/// Add or update timestamp in a filter list file.
/// - `use_hash`: if true, use `#` prefix (for localhost/hosts files), otherwise `!`
///
/// If timestamp exists, updates it in place. If not, inserts after line 1.
/// Returns true if the file was modified.
pub fn add_timestamp(filename: &Path, use_hash: bool, quiet: bool, no_color: bool) -> io::Result<bool> {
    let content = fs::read_to_string(filename)?;
    if content.is_empty() {
        return Ok(false);
    }

    let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let prefix = if use_hash { "#" } else { "!" };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let timestamp = format_timestamp_utc(now);

    let lines: Vec<&str> = content.lines().collect();
    let timestamp_idx = lines.iter().position(|line| is_timestamp_line(line));

    // Extract old timestamp for display
    let old_timestamp: Option<String> = timestamp_idx.and_then(|idx| {
        let line = lines[idx];
        line.find(':').map(|pos| line[pos + 1..].trim().to_string())
    });

    let result_lines: Vec<String> = if let Some(idx) = timestamp_idx {
        // Update existing timestamp, preserving label
        lines.iter()
            .enumerate()
            .map(|(i, line)| {
                if i == idx {
                    if let Some(colon_pos) = line.find(':') {
                        format!("{}: {}", &line[..colon_pos], timestamp)
                    } else {
                        format!("{} Last modified: {}", prefix, timestamp)
                    }
                } else {
                    line.to_string()
                }
            })
            .collect()
    } else {
        // Insert after line 1 (before checksum if present)
        let timestamp_line = format!("{} Last modified: {}", prefix, timestamp);
        let mut result: Vec<String> = Vec::with_capacity(lines.len() + 1);
        for (i, line) in lines.iter().enumerate() {
            result.push(line.to_string());
            if i == 0 {
                result.push(timestamp_line.clone());
            }
        }
        result
    };

    let mut result = result_lines.join(line_ending);
    if content.ends_with('\n') || content.ends_with("\r\n") {
        result.push_str(line_ending);
    }

    if result == content {
        return Ok(false);
    }

    fs::write(filename, &result)?;

    if !quiet {
        if no_color {
            if let Some(ref old) = old_timestamp {
                println!("Timestamp: {} -> {} {}", old, timestamp, filename.display());
            } else {
                println!("Timestamp: {} {}", timestamp, filename.display());
            }
        } else if let Some(ref old) = old_timestamp {
            println!("{} {} -> {} {}", "Timestamp:".bold(), old.red(), timestamp.green(), filename.display());
        } else {
            println!("{} {} {}", "Timestamp:".bold(), timestamp.green(), filename.display());
        }
    }

    Ok(true)
}
