//! Adblock Plus checksum support
//!
//! Calculates and inserts/updates `! Checksum: <base64-md5>` in filter list headers.
//! Uses the standard ABP format: MD5 of normalized content, Base64 without padding.

use std::fs;
use std::io;
use std::path::Path;
use owo_colors::OwoColorize;

use base64::{engine::general_purpose::STANDARD, Engine};
use md5::Context;

/// Check if line is a checksum line (case-insensitive)
#[inline]
fn is_checksum_line(line: &str) -> bool {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix('!').or_else(|| trimmed.strip_prefix('#')) {
        let rest = rest.trim_start();
        // Compare bytes: `rest` is arbitrary list content, so slicing the
        // `&str` at byte 8 panics on a non-ASCII comment (e.g. `! 日本語です`).
        let rb = rest.as_bytes();
        rb.len() >= 8 && rb[..8].eq_ignore_ascii_case(b"checksum")
    } else {
        false
    }
}

/// Calculate ABP-compatible checksum: MD5 of normalized content, Base64 without padding.
/// Normalization: remove \r, collapse consecutive \n.
/// Matches Perl: `$data =~ s/\r//g; $data =~ s/\n+/\n/g; md5_base64(encode_utf8($data))`
#[inline]
fn calculate_checksum(data: &str) -> String {
    let mut hasher = Context::new();
    let mut prev_newline = false;

    for byte in data.bytes() {
        match byte {
            b'\r' => continue,
            b'\n' if prev_newline => continue,
            b'\n' => {
                hasher.consume(b"\n");
                prev_newline = true;
            }
            _ => {
                hasher.consume([byte]);
                prev_newline = false;
            }
        }
    }

    let digest = hasher.finalize();
    let mut encoded = STANDARD.encode(digest.0);
    // Remove trailing '=' padding
    while encoded.ends_with('=') {
        encoded.pop();
    }
    encoded
}

/// Result of checksum verification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChecksumResult {
    /// Checksum matches
    Valid,
    /// Checksum doesn't match (expected, found)
    Invalid { expected: String, found: String },
    /// No checksum line in file
    Missing,
}

/// Verify checksum in a filter list file.
/// Returns the verification result without modifying the file.
pub fn verify_checksum(filename: &Path) -> io::Result<ChecksumResult> {
    let content = fs::read_to_string(filename)?;
    if content.is_empty() {
        return Ok(ChecksumResult::Missing);
    }

    let lines: Vec<&str> = content.lines().collect();

    // Find existing checksum
    let found_checksum = lines.iter()
        .find(|line| is_checksum_line(line))
        .and_then(|line| line.split(':').nth(1))
        .map(|s| s.trim().to_string());

    let Some(found) = found_checksum else {
        return Ok(ChecksumResult::Missing);
    };

    // Calculate expected checksum
    let data_for_hash: String = lines.iter()
        .filter(|line| !is_checksum_line(line))
        .copied()
        .collect::<Vec<_>>()
        .join("\n") + "\n";

    let expected = calculate_checksum(&data_for_hash);

    if expected == found {
        Ok(ChecksumResult::Valid)
    } else {
        Ok(ChecksumResult::Invalid { expected, found })
    }
}

/// Add or update checksum in a filter list file.
/// - `use_hash`: if true, use `#` prefix (for localhost/hosts files), otherwise `!`
///
/// Returns `Ok(None)` if the file was unchanged (checksum already correct),
/// or `Ok(Some(checksum))` with the written checksum if the file was modified.
pub fn add_checksum(filename: &Path, use_hash: bool, quiet: bool, no_color: bool) -> io::Result<Option<String>> {
    let content = fs::read_to_string(filename)?;
    if content.is_empty() {
        return Ok(None);
    }

    // Detect line ending from file
    let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let prefix = if use_hash { "#" } else { "!" };

    // Split into lines, find existing checksum line index
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();
    let checksum_idx = lines.iter().position(|line| is_checksum_line(line));

    // Extract old checksum for display
    let old_checksum: Option<String> = checksum_idx.and_then(|idx| {
        lines[idx].split(':')
            .nth(1)
            .map(|s| s.trim().to_string())
    });

    // Build content without checksum for hashing
    let mut without_checksum: Vec<&str> = Vec::with_capacity(line_count);
    without_checksum.extend(lines.iter()
        .copied()
        .filter(|line| !is_checksum_line(line)));

    let data_for_hash = without_checksum.join("\n") + "\n";
    let checksum = calculate_checksum(&data_for_hash);
    let checksum_line = format!("{} Checksum: {}", prefix, checksum);

    // Check if checksum would be unchanged
    if let Some(idx) = checksum_idx {
        if lines[idx].ends_with(&checksum) {
            return Ok(None);
        }
    }

    // Build result - either replace existing or insert new
    let result_lines: Vec<String> = if let Some(idx) = checksum_idx {
        // Replace existing checksum line
        lines.iter()
            .enumerate()
            .map(|(i, line)| {
                if i == idx {
                    checksum_line.clone()
                } else {
                    line.to_string()
                }
            })
            .collect()
    } else {
        // Insert checksum after line 1 (matches Perl: $data =~ s/(\r?\n)/$1! Checksum: $checksum$1/)
        let mut result: Vec<String> = Vec::with_capacity(without_checksum.len() + 1);
        for (i, line) in without_checksum.iter().enumerate() {
            result.push(line.to_string());
            if i == 0 {
                result.push(checksum_line.clone());
            }
        }
        result
    };

    let mut result = result_lines.join(line_ending);
    if content.ends_with('\n') || content.ends_with("\r\n") {
        result.push_str(line_ending);
    }

    // Only write if changed
    if result == content {
        return Ok(None);
    }

    fs::write(filename, &result)?;

    if !quiet {
        if no_color {
            if let Some(ref old) = old_checksum {
                println!("Checksum: {} -> {} {}", old, checksum, filename.display());
            } else {
                println!("Checksum: {} {}", checksum, filename.display());
            }
        } else if let Some(ref old) = old_checksum {
            println!("{} {} -> {} {}", "Checksum:".bold(), old.red(), checksum.green(), filename.display());
        } else {
            println!("{} {} {}", "Checksum:".bold(), checksum.green(), filename.display());
        }
    }

    Ok(Some(checksum))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_checksum_line() {
        assert!(is_checksum_line("! Checksum: abc123"));
        assert!(is_checksum_line("!Checksum: abc123"));
        assert!(is_checksum_line("# checksum: abc123"));
        assert!(!is_checksum_line("! Title: EasyList"));
        assert!(!is_checksum_line("||example.com^"));
        // Non-ASCII comments must not panic: slicing the &str at byte 8 used
        // to land mid-character. These run over every line of every list.
        assert!(!is_checksum_line("! \u{65e5}\u{672c}\u{8a9e}\u{3067}\u{3059}"));
        assert!(!is_checksum_line("! \u{421}\u{43f}\u{438}\u{441}\u{43e}\u{43a}"));
        assert!(!is_checksum_line("# \u{4f8b}\u{4f8b}\u{4f8b}"));
        assert!(!is_checksum_line("! \u{1f600}\u{1f600}\u{1f600}"));
    }

    /// A list file in the temp dir, removed on drop.
    struct TempList(std::path::PathBuf);

    impl TempList {
        fn new(name: &str, content: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("fop-test-checksum-{}-{}", std::process::id(), name));
            fs::write(&path, content).unwrap();
            TempList(path)
        }

        fn read(&self) -> String {
            fs::read_to_string(&self.0).unwrap()
        }
    }

    impl Drop for TempList {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    // Expected values come from an independent implementation of the ABP
    // algorithm (Python: strip \r, collapse \n+, MD5, Base64, strip `=`),
    // not from this code.
    const TEST_LIST: &str = "[Adblock Plus 2.0]\n! Title: Test\n||example.com^\n##.ad\n";
    const TEST_LIST_CHECKSUM: &str = "VcaUCVKDkdNs7YZ5nP756Q";

    #[test]
    fn test_calculate_checksum_known_answers() {
        assert_eq!(calculate_checksum(""), "1B2M2Y8AsgTpgAmY7PhCfg");
        assert_eq!(calculate_checksum("abc\n"), "C+6JsHokjifIP8PVlRITwQ");
        assert_eq!(calculate_checksum(TEST_LIST), TEST_LIST_CHECKSUM);
        // Hashed as UTF-8 bytes
        assert_eq!(calculate_checksum("! Titel: \u{dc}bersicht\n||b\u{fc}cher.de^\n"),
                   "ml87H+wtk36O23mwspnPaQ");
    }

    #[test]
    fn test_calculate_checksum_normalization() {
        // \r is dropped and blank lines collapse, so line endings and
        // blank-line edits never change the checksum
        let crlf = "[Adblock Plus 2.0]\r\n! Title: Test\r\n||example.com^\r\n##.ad\r\n";
        let blanks = "[Adblock Plus 2.0]\n\n\n! Title: Test\n||example.com^\n\r\n\n##.ad\n\n";
        assert_eq!(calculate_checksum(crlf), TEST_LIST_CHECKSUM);
        assert_eq!(calculate_checksum(blanks), TEST_LIST_CHECKSUM);
        // Any other change does
        assert_ne!(calculate_checksum("[Adblock Plus 2.0]\n! Title: Test\n||example.com^\n##.ads\n"),
                   TEST_LIST_CHECKSUM);
        // Unpadded: MD5 is 16 bytes, so Base64 would end in `==`
        assert_eq!(TEST_LIST_CHECKSUM.len(), 22);
    }

    #[test]
    fn test_add_checksum_inserts_after_header() {
        let list = TempList::new("insert", TEST_LIST);
        assert_eq!(add_checksum(&list.0, false, true, true).unwrap(),
                   Some(TEST_LIST_CHECKSUM.to_string()));
        assert_eq!(list.read(), format!(
            "[Adblock Plus 2.0]\n! Checksum: {}\n! Title: Test\n||example.com^\n##.ad\n",
            TEST_LIST_CHECKSUM));
        assert_eq!(verify_checksum(&list.0).unwrap(), ChecksumResult::Valid);

        // Idempotent: a correct checksum leaves the file alone
        let before = list.read();
        assert_eq!(add_checksum(&list.0, false, true, true).unwrap(), None);
        assert_eq!(list.read(), before);
    }

    #[test]
    fn test_add_checksum_replaces_stale_in_place() {
        let list = TempList::new("stale",
            "[Adblock Plus 2.0]\n! Title: Test\n! Checksum: staleValue\n||example.com^\n##.ad\n");
        assert_eq!(verify_checksum(&list.0).unwrap(), ChecksumResult::Invalid {
            expected: TEST_LIST_CHECKSUM.to_string(),
            found: "staleValue".to_string(),
        });
        assert_eq!(add_checksum(&list.0, false, true, true).unwrap(),
                   Some(TEST_LIST_CHECKSUM.to_string()));
        // Same position, not moved to line 2
        assert_eq!(list.read(), format!(
            "[Adblock Plus 2.0]\n! Title: Test\n! Checksum: {}\n||example.com^\n##.ad\n",
            TEST_LIST_CHECKSUM));
        assert_eq!(verify_checksum(&list.0).unwrap(), ChecksumResult::Valid);
    }

    #[test]
    fn test_add_checksum_keeps_line_endings_and_prefix() {
        // CRLF file, hosts-style `#` prefix, no trailing newline
        let list = TempList::new("crlf", "# Title: Hosts\r\n127.0.0.1 ads.example.com");
        let checksum = add_checksum(&list.0, true, true, true).unwrap().unwrap();
        assert_eq!(checksum, calculate_checksum("# Title: Hosts\n127.0.0.1 ads.example.com\n"));
        assert_eq!(list.read(),
                   format!("# Title: Hosts\r\n# Checksum: {}\r\n127.0.0.1 ads.example.com", checksum));
        assert_eq!(verify_checksum(&list.0).unwrap(), ChecksumResult::Valid);
    }

    #[test]
    fn test_verify_checksum_detects_edits() {
        let list = TempList::new("edit", TEST_LIST);
        add_checksum(&list.0, false, true, true).unwrap();
        fs::write(&list.0, list.read().replace("##.ad", "##.ads")).unwrap();
        assert!(matches!(verify_checksum(&list.0).unwrap(),
                         ChecksumResult::Invalid { found, .. } if found == TEST_LIST_CHECKSUM));

        // Blank-line and line-ending changes are not edits
        fs::write(&list.0, format!(
            "[Adblock Plus 2.0]\r\n! Checksum: {}\r\n\r\n! Title: Test\r\n||example.com^\r\n##.ad\r\n",
            TEST_LIST_CHECKSUM)).unwrap();
        assert_eq!(verify_checksum(&list.0).unwrap(), ChecksumResult::Valid);
    }

    #[test]
    fn test_checksum_missing_and_empty() {
        let list = TempList::new("missing", TEST_LIST);
        assert_eq!(verify_checksum(&list.0).unwrap(), ChecksumResult::Missing);

        let empty = TempList::new("empty", "");
        assert_eq!(verify_checksum(&empty.0).unwrap(), ChecksumResult::Missing);
        assert_eq!(add_checksum(&empty.0, false, true, true).unwrap(), None);
        assert_eq!(empty.read(), "");
    }
}
