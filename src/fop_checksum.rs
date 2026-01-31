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
        rest.trim_start().to_ascii_lowercase().starts_with("checksum")
    } else {
        false
    }
}

/// Calculate ABP-compatible checksum: MD5 of normalized content, Base64 without padding.
/// Normalization: remove \r, collapse consecutive \n.
#[inline]
fn calculate_checksum(data: &str) -> String {
    let mut hasher = Context::new();
    let mut last_was_newline = false;
    let mut pending_newline = false;

    for ch in data.chars() {
        match ch {
            '\r' => continue,
            '\n' => {
                if !last_was_newline {
                    pending_newline = true;
                    last_was_newline = true;
                }
            }
            _ => {
                if pending_newline {
                    hasher.consume(b"\n");
                    pending_newline = false;
                }
                let mut buf = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buf);
                hasher.consume(encoded.as_bytes());
                last_was_newline = false;
            }
        }
    }

    if pending_newline {
        hasher.consume(b"\n");
    }

    let digest = hasher.finalize();
    let mut encoded = STANDARD.encode(digest.0);
    // Remove trailing '=' padding
    while encoded.ends_with('=') {
        encoded.pop();
    }
    encoded
}

/// Add or update checksum in a filter list file.
/// - `use_hash`: if true, use `#` prefix (for localhost/hosts files), otherwise `!`
///
/// Returns true if the file was modified.
pub fn add_checksum(filename: &Path, use_hash: bool, quiet: bool, no_color: bool) -> io::Result<bool> {
    let content = fs::read_to_string(filename)?;
    if content.is_empty() {
        return Ok(false);
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
            return Ok(false);
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
        // Insert new checksum line after header/timestamp
        let mut insert_idx = 0;
        for (i, line) in without_checksum.iter().enumerate() {
            let trimmed = line.trim();
            // After [Adblock...] header
            if i == 0 && trimmed.starts_with('[') && trimmed.ends_with(']') {
                insert_idx = 1;
                continue;
            }
            // After timestamp lines
            let lower = trimmed.to_ascii_lowercase();
            if lower.contains("last modified:") || lower.contains("last updated:") {
                insert_idx = i + 1;
                break;
            }
            if i >= 10 {
                break;
            }
        }

        let mut result: Vec<String> = Vec::with_capacity(without_checksum.len() + 1);
        for (i, line) in without_checksum.iter().enumerate() {
            if i == insert_idx {
                result.push(checksum_line.clone());
            }
            result.push(line.to_string());
        }
        // Handle insert at position 0
        if insert_idx == 0 && !result.iter().any(|l| l == &checksum_line) {
            result.insert(0, checksum_line);
        }
        result
    };

    let mut result = result_lines.join(line_ending);
    if content.ends_with('\n') || content.ends_with("\r\n") {
        result.push_str(line_ending);
    }

    // Only write if changed
    if result == content {
        return Ok(false);
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

    Ok(true)
}