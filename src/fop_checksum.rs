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
        rest.len() >= 8 && rest[..8].eq_ignore_ascii_case("checksum")
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