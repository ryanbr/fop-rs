//! Filter sorting and tidying logic
//!
//! Copyright (C) 2025 FanboyNZ (FOP Rust)
//! https://github.com/ryanbr/fop-rs
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use ahash::AHashSet as HashSet;
use regex::Regex;

use crate::{
    ELEMENT_PATTERN, FOPPY_ELEMENT_PATTERN, ELEMENT_DOMAIN_PATTERN,
    FOPPY_ELEMENT_DOMAIN_PATTERN, FILTER_DOMAIN_PATTERN, OPTION_PATTERN,
    LOCALHOST_PATTERN, SHORT_DOMAIN_PATTERN, DOMAIN_EXTRACT_PATTERN,
    IP_ADDRESS_PATTERN, REGEX_ELEMENT_PATTERN,
    ATTRIBUTE_VALUE_PATTERN, TREE_SELECTOR, REMOVAL_PATTERN,
    PSEUDO_PATTERN, UNICODE_SELECTOR,
    KNOWN_OPTIONS, IGNORE_DOMAINS, UBO_CONVERSIONS, write_warning,
};

/// Check if line is a TLD-only pattern (e.g. .com, ||.net^)
/// Replaces regex: r"^(\|\||[|])?\.([a-z]{2,})\^?$"
#[inline]
fn is_tld_only(line: &str) -> bool {
    let s = if line.starts_with("||") {
        &line[2..]
    } else if line.starts_with('|') {
        &line[1..]
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
    pub disable_domain_limit: bool,
}

// =============================================================================
// UBO Option Conversion
// =============================================================================

/// Convert uBO-specific options to standard ABP options
pub(crate) fn convert_ubo_options(options: Vec<String>) -> Vec<String> {
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
pub(crate) fn sort_domains(domains: &mut Vec<String>) {
    domains.sort_unstable_by(|a, b| a.trim_start_matches('~').cmp(b.trim_start_matches('~')));
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
pub(crate) fn filter_tidy(filter_in: &str, convert_ubo: bool) -> String {

    // Skip filters with regex values in options (contain =/.../ patterns)
    // ||example.com$removeparam=/^\\$ja=/
    // ||example.com$removeparam=/regex/
    if filter_in.contains("=/") && filter_in.contains("$") {
        return filter_in.to_string();
    }
    
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
            let is_valid = stripped == "*" 
                || (!has_dot && len >= 2)
                || (has_dot && len >= 4);
            if !is_valid {
                invalid_domains.push((*d).to_string());
            } else {
                valid_domains.push((*d).to_string());
            }
        }

        if !invalid_domains.is_empty() {
            write_warning(&format!(
                "Removed invalid domain(s) from cosmetic rule: {} | Rule: {}{}{}",
                invalid_domains.join(", "), domains, separator, selector
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

        new_domains.sort_unstable_by(|a, b| {
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
// Main Sorting Function
// =============================================================================

/// Sort the sections of a filter file and save modifications
pub fn fop_sort(filename: &Path, config: &SortConfig) -> io::Result<()> {
    let temp_file = filename.with_extension("temp");
    const CHECK_LINES: usize = 10;

    let input = match File::open(filename) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Cannot open {}: {}", filename.display(), e);
            return Ok(());
        }
    };
    let reader = BufReader::new(input);
    let mut output = match File::create(&temp_file) {
        Ok(f) => BufWriter::with_capacity(64 * 1024, f),
        Err(e) => {
            eprintln!("Cannot create temp file for {}: {}", filename.display(), e);
            return Ok(());
        }
    };

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
                unique.sort_unstable_by(|a, b| {
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
                unique.sort_unstable_by(|a, b| {
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
                unique.sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
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
            if config.keep_empty_lines {
                if !section.is_empty() {
                    write_filters(&mut section, &mut output, element_lines, filter_lines, config.no_sort, config.alt_sort, config.localhost)?;
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
            || (config.localhost && line.starts_with('#') && !config.comment_chars.contains(&"#".to_string()));
        if is_comment
            || line.starts_with("%include")
            || (line.starts_with('[') && line.ends_with(']'))
        {
            if !section.is_empty() {
                write_filters(&mut section, &mut output, element_lines, filter_lines, config.no_sort, config.alt_sort, config.localhost)?;
                lines_checked = 1;
                filter_lines = 0;
                element_lines = 0;
            }
            writeln!(output, "{}", line)?;
            continue;
        }
        
        // Validate localhost entries when in localhost mode
        if config.localhost {
            if !LOCALHOST_PATTERN.is_match(&line) {
                write_warning(&format!(
                    "Removed invalid localhost entry: {}", line
                ));
                continue;
            }
        }

        // Skip filters less than 3 characters
        if line.len() < 3 {
            continue;
        }

        // [$path=/\/(dom|pro)/]rambler.ru##div[style^="order:"][style*="-1"]
        // AdGuard cosmetic rule modifiers - pass through unchanged
        if line.starts_with("[$") {
            section.push(line);
            continue;
        }

        // Handle regex domain rules (uBO) - pass through unchanged
        if REGEX_ELEMENT_PATTERN.is_match(&line) {
            section.push(line);
            continue;
        }

        // Process element hiding rules
        let element_caps = if config.alt_sort {
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
        if !config.disable_domain_limit && line.len() <= 7 && SHORT_DOMAIN_PATTERN.is_match(&line) {
            if let Some(caps) = DOMAIN_EXTRACT_PATTERN.captures(&line) {
                let domain = &caps[1];
                if !IGNORE_DOMAINS.contains(domain) {
                    write_warning(&format!(
                        "Skipped short domain rule: {} (domain: {})", line, domain
                    ));
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

                if !config.ignore_dot_domains && !is_ip && !has_wildcard && !domain.contains('.') && !domain.starts_with('~') {
                    write_warning(&format!(
                        "Skipped network rule without dot in domain: {} (domain: {})", line, domain
                    ));
                    continue;
                }
            }
        }

        // Remove TLD-only patterns
        if is_tld_only(&line) {
            write_warning(&format!(
                "Removed overly broad TLD-only rule: {}", line
            ));
            continue;
        }

        if lines_checked <= CHECK_LINES {
            filter_lines += 1;
            lines_checked += 1;
        }

        let tidied = filter_tidy(&line, config.convert_ubo);
        section.push(tidied);
    }

    // Write remaining filters
    if !section.is_empty() {
        write_filters(&mut section, &mut output, element_lines, filter_lines, config.no_sort, config.alt_sort, config.localhost)?;
    }

    drop(output);

    // Compare files and replace if different
    let original_content = fs::read(filename)?;
    let new_content = fs::read(&temp_file)?;

    if original_content != new_content {
        // Create backup if requested
        if config.backup {
            let backup_file = filename.with_extension("backup");
            fs::copy(filename, &backup_file)?;
        }
        fs::rename(&temp_file, filename)?;
        println!("Sorted: {}", filename.display());
    } else {
        fs::remove_file(&temp_file)?;
    }


    Ok(())
}
