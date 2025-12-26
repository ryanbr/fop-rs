//! Typo detection and correction for cosmetic filter rules
//!
//! Common typos:
//! - ###.class ? ##.class
//! - ##..class ? ##.class
//! - domain#.class ? domain##.class
//! - domain,,domain##.ad ? domain,domain##.ad

use regex::Regex;
use std::sync::LazyLock;

// =============================================================================
// Cosmetic Typo Patterns
// =============================================================================

/// Cosmetic rule with extra # (###.class or domain###.class)
static EXTRA_HASH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([^#]*)(###+)([.#\[\*])").unwrap()
});

/// Single # that should be ## (domain#.class)
static SINGLE_HASH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([^#]+)#([.#\[\*][a-zA-Z])").unwrap()
});

/// Double dot in cosmetic selector (##..class)
static DOUBLE_DOT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(##)\.\.([a-zA-Z])").unwrap()
});

/// Double comma in domain list (domain,,domain)
static DOUBLE_COMMA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r",,+").unwrap()
});

/// Trailing comma before ## (domain,##.ad)
static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r",+(#[@?$%]?#)").unwrap()
});

/// Leading comma after domain start (,domain##.ad)
static LEADING_COMMA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^,+([a-zA-Z])").unwrap()
});

/// Wrong cosmetic domain separator (using | instead of ,)
static WRONG_COSMETIC_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| 
    Regex::new(r"^([a-zA-Z0-9~][a-zA-Z0-9\.\-,]*\.[a-zA-Z]{2,})\|([a-zA-Z0-9~][a-zA-Z0-9\.\-\|,]*)(#[@?$%]?#|#@[$%?]#|#\+js)").unwrap()
);

// =============================================================================
// Network Rule Typo Patterns
// =============================================================================

/// Triple $$$ before domain= ($$$domain= ? $domain=)
static TRIPLE_DOLLAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$\$domain=").unwrap());

/// Double $$ before domain= ($$domain= ? $domain=)
static DOUBLE_DOLLAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$domain=").unwrap());

/// Missing $ before domain= (after common file extensions)
static MISSING_DOLLAR: LazyLock<Regex> = LazyLock::new(|| 
    Regex::new(r"(\.(js|css|html|php|json|xml|gif|png|jpg|jpeg|svg|webp|woff2?|ttf|eot|mp[34]|m3u8)|\^)domain=([a-zA-Z0-9][\w\-]*\.[a-zA-Z]{2,})").unwrap()
);

/// Wrong domain separator (using , instead of |)
static WRONG_DOMAIN_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| 
    Regex::new(r"(domain=|\|)([a-zA-Z0-9~\*][a-zA-Z0-9\.\-\*]*\.[a-zA-Z]{2,}),([a-zA-Z0-9~\*])").unwrap()
);

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

    // Network rules - check for $$ and $$$ typos
    if line.starts_with("||") || line.starts_with('|') || line.starts_with("@@") || line.contains("$domain=") || line.contains(",domain=") {
        // Check for $$$ before domain=
        if TRIPLE_DOLLAR.is_match(line) {
            let fixed = TRIPLE_DOLLAR.replace(line, "$$domain=").to_string();
            return Some(Typo { original: line.to_string(), fixed, description: "Triple $ ($$$ ? $)".to_string() });
        }

        // Check for $$ before domain=
        if DOUBLE_DOLLAR.is_match(line) {
            let fixed = DOUBLE_DOLLAR.replace(line, "$$domain=").to_string();
            return Some(Typo { original: line.to_string(), fixed, description: "Double $ ($$ ? $)".to_string() });
        }

        // Check for missing $ before domain=
        if MISSING_DOLLAR.is_match(line) {
            let fixed = MISSING_DOLLAR.replace(line, "$1$$domain=$3").to_string();
            return Some(Typo { original: line.to_string(), fixed, description: "Missing $ before domain=".to_string() });
        }

        // Check for wrong domain separator (, instead of |)
        if WRONG_DOMAIN_SEPARATOR.is_match(line) {
            let fixed = WRONG_DOMAIN_SEPARATOR.replace(line, "$1$2|$3").to_string();
            return Some(Typo { original: line.to_string(), fixed, description: "Wrong domain separator (, ? |)".to_string() });
        }

        return None;  // No cosmetic typos in network rules
    }

    // Skip non-cosmetic rules (no # at all)
    if !line.contains('#') {
        return None;
    }

    // Check for wrong cosmetic domain separator (| instead of ,)
    if WRONG_COSMETIC_SEPARATOR.is_match(line) {
        let fixed = WRONG_COSMETIC_SEPARATOR.replace(line, "$1,$2$3").to_string();
        return Some(Typo { original: line.to_string(), fixed, description: "Wrong cosmetic separator (| ? ,)".to_string() });
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

    #[test]
    fn test_triple_dollar() {
        let result = detect_typo("@@||example.com/cc.js$$$domain=asket.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "@@||example.com/cc.js$domain=asket.com");
    }

    #[test]
    fn test_double_dollar() {
        let result = detect_typo("@@||example.com/cc.js$$domain=asket.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "@@||example.com/cc.js$domain=asket.com");
        
        let result = detect_typo("||example.com/ad.js$$domain=test.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "||example.com/ad.js$domain=test.com");
    }

    #[test]
    fn test_missing_dollar() {
        let result = detect_typo("@@||example.com/cc.jsdomain=asket.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "@@||example.com/cc.js$domain=asket.com");
        
        // With ^ separator
        let result = detect_typo("@@||example.com/cc.js^domain=asket.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "@@||example.com/cc.js^$domain=asket.com");
        
        // Valid should not match
        let result = detect_typo("@@||example.com/cc.js$domain=asket.com");
        assert!(result.is_none());

        // No domain after domain= should not match
        let result = detect_typo("@@||example.com/cc.jsdomain=");
        assert!(result.is_none());
    }

    #[test]
    fn test_wrong_cosmetic_separator() {
        // Single pipe
        let result = detect_typo("domain.com|domain2.com##.test");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "domain.com,domain2.com##.test");
        
        // Multiple pipes (fix_all_typos handles iteratively)
        let (fixed, _) = fix_all_typos("domain.com|domain2.com|domain3.com##.test");
        assert_eq!(fixed, "domain.com,domain2.com,domain3.com##.test");
        
        // Mixed separators
        let (fixed, _) = fix_all_typos("domain.com|domain2.com,domain3.com##.test");
        assert_eq!(fixed, "domain.com,domain2.com,domain3.com##.test");
        
        // With ##+js
        let (fixed, _) = fix_all_typos("domain3.com|domain2.com,domain1.com##+js(nowolf)");
        assert_eq!(fixed, "domain3.com,domain2.com,domain1.com##+js(nowolf)");
        
        // Valid comma separator should not match
        let result = detect_typo("domain.com,domain2.com##.test");
        assert!(result.is_none());
    }
    
    #[test]
    fn test_wrong_domain_separator() {
        // Single comma
        let result = detect_typo("||example.com$domain=site1.com,site2.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "||example.com$domain=site1.com|site2.com");
        
        // Multiple commas (fix_all_typos handles iteratively)
        let (fixed, fixes) = fix_all_typos("||example.com$3p,domain=a.com,b.com,c.com");
        assert_eq!(fixed, "||example.com$3p,domain=a.com|b.com|c.com");
        assert_eq!(fixes.len(), 2);
        
        // Mixed separators
        let (fixed, _) = fix_all_typos("*.global/$3p,domain=animepahe.si,daddyhd.com|soap2day.day");
        assert_eq!(fixed, "*.global/$3p,domain=animepahe.si|daddyhd.com|soap2day.day");
        
        // Valid pipe separator should not match
        let result = detect_typo("||example.com$domain=site1.com|site2.com");
        assert!(result.is_none());
    }
}