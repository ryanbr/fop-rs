//! Typo detection and correction for cosmetic filter rules
//!
//! Common typos:
//! - ###.class ? ##.class
//! - ##..class ? ##.class
//! - domain#.class ? domain##.class
//! - domain,,domain##.ad ? domain,domain##.ad

use regex::Regex;
use once_cell::sync::Lazy;

// =============================================================================
// Cosmetic Typo Patterns
// =============================================================================

/// Cosmetic rule with extra # (###.class or domain###.class)
static EXTRA_HASH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([^#]*)(###+)([.#\[\*])").unwrap()
});

/// Single # that should be ## (domain#.class)
static SINGLE_HASH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([^#]+)#([.#\[\*][a-zA-Z])").unwrap()
});

/// Double dot in cosmetic selector (##..class)
static DOUBLE_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(##)\.\.([a-zA-Z])").unwrap()
});

/// Double comma in domain list (domain,,domain)
static DOUBLE_COMMA: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r",,+").unwrap()
});

/// Trailing comma before ## (domain,##.ad)
static TRAILING_COMMA: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r",+(#[@?$%]?#)").unwrap()
});

/// Leading comma after domain start (,domain##.ad)
static LEADING_COMMA: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^,+([a-zA-Z])").unwrap()
});

// =============================================================================
// Typo Detection
// =============================================================================

#[derive(Debug, Clone)]
pub struct Typo {
    pub original: String,
    pub fixed: String,
    pub description: String,
}

/// Helper to create Typo if regex matches and changes line
#[inline]
fn try_fix(line: &str, pattern: &Regex, replacement: &str, description: &str) -> Option<Typo> {
    let fixed = pattern.replace_all(line, replacement);
    if fixed != line {
        return Some(Typo {
            original: line.to_string(),
            fixed: fixed.to_string(),
            description: description.to_string(),
        });
    }
    None
}

/// Check a cosmetic rule for typos
#[inline]
pub fn detect_typo(line: &str) -> Option<Typo> {
    // Skip comments, empty lines, special directives, short lines
    if line.len() < 4
        || line.starts_with('!')
        || line.starts_with('[')
        || line.starts_with('%')
    {
        return None;
    }

    // Skip network rules (no # at all, or starts with || or |)
    if !line.contains('#') || line.starts_with("||") || line.starts_with('|') {
        return None;
    }

    // Check for extra # (### ? ##)
    if let Some(caps) = EXTRA_HASH.captures(line) {
        let hashes = &caps[2];
        if hashes.len() > 2 {
            let fixed = EXTRA_HASH.replace(line, "${1}##${3}").to_string();
            return Some(Typo {
                original: line.to_string(),
                fixed,
                description: format!("Extra # ({} ? ##)", hashes),
            });
        }
    }

    // Check for single # that should be ## (domain#.ad ? domain##.ad)
    if !line.contains("##") {
        if let Some(typo) = try_fix(line, &SINGLE_HASH, "${1}##${2}", "Single # (# ? ##)") {
            return Some(typo);
        }
    }

    // Chain remaining checks
    try_fix(line, &DOUBLE_DOT, "${1}.${2}", "Double dot (.. ? .)")
        .or_else(|| try_fix(line, &DOUBLE_COMMA, ",", "Double comma (,, ? ,)"))
        .or_else(|| try_fix(line, &TRAILING_COMMA, "${1}", "Trailing comma before ##"))
        .or_else(|| try_fix(line, &LEADING_COMMA, "${1}", "Leading comma removed"))
}

/// Fix all typos in a line (iterates until no more fixes)
pub fn fix_all_typos(line: &str) -> (String, Vec<String>) {
    let mut current = line.to_string();
    let mut all_fixes = Vec::new();

    // Limit iterations to prevent infinite loops
    for _ in 0..10 {
        match detect_typo(&current) {
            Some(typo) => {
                all_fixes.push(typo.description);
                current = typo.fixed;
            }
            None => break,
        }
    }

    (current, all_fixes)
}

// =============================================================================
// Git Addition Checking (for --fix-typos-on-add)
// =============================================================================

#[derive(Debug, Clone)]
pub struct Addition {
    pub file: String,
    pub line_num: usize,
    pub content: String,
}

/// Check added lines for typos
pub fn check_additions(additions: &[Addition]) -> Vec<(Addition, Typo)> {
    let mut results = Vec::new();
    for add in additions {
        if let Some(typo) = detect_typo(&add.content) {
            results.push((add.clone(), typo));
        }
    }
    results
}

/// Report typos in additions (formatted output)
pub fn report_addition_typos(typos: &[(Addition, Typo)], no_color: bool) {
    if typos.is_empty() {
        return;
    }
    
    println!("\nTypos found in added lines:");
    for (add, typo) in typos {
        if no_color {
            println!("  {}:{}: {} ? {}", add.file, add.line_num, typo.original, typo.fixed);
        } else {
            use colored::Colorize;
            println!("  {}:{}: {} ? {}", 
                add.file.cyan(), 
                add.line_num,
                typo.original.red(), 
                typo.fixed.green()
            );
        }
        println!("    ({})", typo.description);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extra_hash() {
        let typo = detect_typo("###.ad-banner").unwrap();
        assert_eq!(typo.fixed, "##.ad-banner");
        
        let typo = detect_typo("example.com###.ad").unwrap();
        assert_eq!(typo.fixed, "example.com##.ad");
        
        let typo = detect_typo("####.ad").unwrap();
        assert_eq!(typo.fixed, "##.ad");
    }

    #[test]
    fn test_single_hash() {
        let typo = detect_typo("domain#.ad").unwrap();
        assert_eq!(typo.fixed, "domain##.ad");
        
        let typo = detect_typo("example.com#.banner").unwrap();
        assert_eq!(typo.fixed, "example.com##.banner");
        
        let typo = detect_typo("domain#[class]").unwrap();
        assert_eq!(typo.fixed, "domain##[class]");
    }

    #[test]
    fn test_double_dot() {
        let typo = detect_typo("##..ad-class").unwrap();
        assert_eq!(typo.fixed, "##.ad-class");
    }

    #[test]
    fn test_double_comma() {
        let typo = detect_typo("example.com,,test.com##.ad").unwrap();
        assert_eq!(typo.fixed, "example.com,test.com##.ad");
    }
    
    #[test]
    fn test_triple_comma() {
        let typo = detect_typo("a,,,b##.ad").unwrap();
        assert_eq!(typo.fixed, "a,b##.ad");
    }

    #[test]
    fn test_trailing_comma() {
        let typo = detect_typo("example.com,##.ad").unwrap();
        assert_eq!(typo.fixed, "example.com##.ad");
    }

    #[test]
    fn test_leading_comma() {
        let typo = detect_typo(",example.com##.ad").unwrap();
        assert_eq!(typo.fixed, "example.com##.ad");
    }

    #[test]
    fn test_no_typo() {
        assert!(detect_typo("##.ad-banner").is_none());
        assert!(detect_typo("example.com##.ad").is_none());
        assert!(detect_typo("! comment").is_none());
        assert!(detect_typo("||example.com^").is_none());
        assert!(detect_typo("|https://example.com").is_none());
    }

    #[test]
    fn test_fix_all_typos() {
        // Multiple typos: ### and ..
        let (fixed, fixes) = fix_all_typos("###..ad");
        assert_eq!(fixed, "##.ad");
        assert_eq!(fixes.len(), 2);
        
        // Triple comma + single hash
        let (fixed, fixes) = fix_all_typos("domain,,,b#.ad");
        assert_eq!(fixed, "domain,b##.ad");
        assert_eq!(fixes.len(), 2);
    }
    
    #[test]
    fn test_extended_selectors_preserved() {
        // These should not be treated as typos
        assert!(detect_typo("domain##.ad:has(.banner)").is_none());
        assert!(detect_typo("domain##+js(aopr, ads)").is_none());
    }
}