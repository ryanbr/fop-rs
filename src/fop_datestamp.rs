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
    // Compare bytes: `trimmed` is arbitrary list content, so slicing the
    // `&str` at byte 8 panics on a non-ASCII comment (e.g. `! 日本語です`).
    let tb = trimmed.as_bytes();
    tb.len() >= 8 && tb[..8].eq_ignore_ascii_case(b"version:")
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent civil-from-days (Howard Hinnant's algorithm): a different
    /// method from `decompose_utc`'s year/month walk, used as its oracle.
    fn civil_from_days(z: i64) -> (i64, u32, u32) {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let y = yoe + era * 400 + i64::from(m <= 2);
        (y, m, d)
    }

    #[test]
    fn test_is_leap_year() {
        for year in [1972, 1996, 2000, 2004, 2024, 2400] {
            assert!(is_leap_year(year), "{} is a leap year", year);
        }
        // Century years are leap years only when divisible by 400
        for year in [1970, 1900, 2023, 2025, 2100, 2200, 2300] {
            assert!(!is_leap_year(year), "{} is not a leap year", year);
        }
    }

    #[test]
    fn test_decompose_utc_matches_calendar() {
        // Every day from 1970 through 2400, so both century rules (2100
        // skips its leap day, 2000 and 2400 keep theirs), at the first and
        // last minute of the day
        let last_day = 157_419; // 2400-12-31
        for day in 0..=last_day {
            let (y, m, d) = civil_from_days(day);
            let expected = (y as u64, m as usize - 1, u64::from(d));
            for (secs, hh, mm) in [(0, 0, 0), (86_399, 23, 59)] {
                let (year, month, dom, hours, minutes) = decompose_utc(day as u64 * 86_400 + secs);
                assert_eq!((year, month, dom), expected, "day {}", day);
                assert_eq!((hours, minutes), (hh, mm), "day {}", day);
            }
        }
    }

    #[test]
    fn test_format_timestamp_and_version() {
        // Expected values from Python's datetime, not from this code
        let cases = [
            (0, "1 Jan 1970 00:00 UTC", "197001010000"),
            (951_782_400, "29 Feb 2000 00:00 UTC", "200002290000"),
            (951_868_800, "1 Mar 2000 00:00 UTC", "200003010000"),
            (1_735_689_599, "31 Dec 2024 23:59 UTC", "202412312359"),
            (1_769_761_860, "30 Jan 2026 08:31 UTC", "202601300831"),
            (4_102_444_800, "1 Jan 2100 00:00 UTC", "210001010000"),
            (4_107_456_000, "28 Feb 2100 00:00 UTC", "210002280000"),
            (4_107_542_400, "1 Mar 2100 00:00 UTC", "210003010000"),
        ];
        for (secs, timestamp, version) in cases {
            assert_eq!(format_timestamp_utc(secs), timestamp, "secs {}", secs);
            assert_eq!(format_version_utc(secs), version, "secs {}", secs);
        }
        // Seconds are truncated, not rounded
        assert_eq!(format_timestamp_utc(1_769_761_860 + 59), "30 Jan 2026 08:31 UTC");
    }

    #[test]
    fn test_is_timestamp_line() {
        assert!(is_timestamp_line("! Last modified: 1 Jan 2026 00:00 UTC"));
        assert!(is_timestamp_line("# Last Updated: 1 Jan 2026"));
        assert!(is_timestamp_line("!LAST MODIFIED:"));
        assert!(!is_timestamp_line("! Last modified 1 Jan 2026"));
        assert!(!is_timestamp_line("! Title: EasyList"));
        assert!(!is_timestamp_line("! \u{65e5}\u{672c}\u{8a9e}\u{3067}\u{3059}"));
    }

    /// Now, as `update_*` and `add_timestamp` see it: they read the clock
    /// themselves, so tests accept either side of a minute boundary.
    fn now_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    #[test]
    fn test_update_timestamp_line() {
        assert_eq!(update_timestamp_line("! Title: EasyList"), None);
        for (line, prefix, keyword) in [
            ("! Last modified: 1 Jan 2020 00:00 UTC", "!", "Last modified"),
            ("# last updated: yesterday", "#", "Last updated"),
            ("  !Last Modified:", "!", "Last modified"),
        ] {
            let before = now_secs();
            let updated = update_timestamp_line(line).unwrap();
            let after = now_secs();
            let accepted = [before, after].map(|t| format!("{} {}: {}", prefix, keyword, format_timestamp_utc(t)));
            assert!(accepted.contains(&updated), "{:?} -> {:?}", line, updated);
        }
    }

    #[test]
    fn test_update_version_line() {
        assert_eq!(update_version_line("! Title: EasyList"), None);
        for (line, prefix) in [("! Version: 202001010000", "!"), ("# version: 1", "#")] {
            let before = now_secs();
            let updated = update_version_line(line).unwrap();
            let after = now_secs();
            let accepted = [before, after].map(|t| format!("{} Version: {}", prefix, format_version_utc(t)));
            assert!(accepted.contains(&updated), "{:?} -> {:?}", line, updated);
        }
    }

    /// Runs `add_timestamp` on `content` and returns (modified, new content,
    /// the timestamps it may have written).
    fn run_add_timestamp(name: &str, content: &str, use_hash: bool) -> (bool, String, [String; 2]) {
        let path = std::env::temp_dir()
            .join(format!("fop-test-datestamp-{}-{}", std::process::id(), name));
        fs::write(&path, content).unwrap();
        let before = now_secs();
        let modified = add_timestamp(&path, use_hash, true, true);
        let after = now_secs();
        let result = fs::read_to_string(&path);
        let _ = fs::remove_file(&path);
        (modified.unwrap(), result.unwrap(), [before, after].map(format_timestamp_utc))
    }

    #[test]
    fn test_add_timestamp_inserts_after_header() {
        let (modified, result, stamps) =
            run_add_timestamp("insert", "[Adblock Plus 2.0]\n! Title: Test\n||example.com^\n", false);
        assert!(modified);
        assert!(stamps.iter().any(|s| result == format!(
            "[Adblock Plus 2.0]\n! Last modified: {}\n! Title: Test\n||example.com^\n", s)), "{:?}", result);
    }

    #[test]
    fn test_add_timestamp_updates_in_place() {
        // Keeps the existing label and position
        let (modified, result, stamps) = run_add_timestamp("update",
            "[Adblock Plus 2.0]\n! Title: Test\n! Last updated: 1 Jan 2020 00:00 UTC\n||example.com^\n", false);
        assert!(modified);
        assert!(stamps.iter().any(|s| result == format!(
            "[Adblock Plus 2.0]\n! Title: Test\n! Last updated: {}\n||example.com^\n", s)), "{:?}", result);
    }

    #[test]
    fn test_add_timestamp_keeps_line_endings_and_prefix() {
        // CRLF, hosts-style `#` prefix, no trailing newline
        let (modified, result, stamps) =
            run_add_timestamp("crlf", "# Title: Hosts\r\n127.0.0.1 ads.example.com", true);
        assert!(modified);
        assert!(stamps.iter().any(|s| result == format!(
            "# Title: Hosts\r\n# Last modified: {}\r\n127.0.0.1 ads.example.com", s)), "{:?}", result);
    }

    #[test]
    fn test_add_timestamp_empty_file() {
        let (modified, result, _) = run_add_timestamp("empty", "", false);
        assert!(!modified);
        assert_eq!(result, "");
    }
}
