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
static EXTRA_HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^#]*)(###+)([.#\[\*])").unwrap());

/// Single # that should be ## (domain#.class)
static SINGLE_HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^#]+)#([.#\[\*][a-zA-Z])").unwrap());

/// Double dot in cosmetic selector (##..class)
static DOUBLE_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(##)\.\.([a-zA-Z])").unwrap());

/// Double comma in domain list (domain,,domain)
static DOUBLE_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",,+").unwrap());

/// Trailing comma before ## (domain,##.ad)
static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",+(#[@?$%]?#)").unwrap());

/// Leading comma after domain start (,domain##.ad)
static LEADING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^,+([a-zA-Z])").unwrap());

/// Detect space after comma in cosmetic domain list (before ## separator only).
/// Uses string splitting rather than regex to avoid matching inside selectors.
fn detect_space_after_comma(line: &str) -> Option<Typo> {
    // Find the ## separator (any variant: ##, #@#, #?#, #$#, #@$#, #%#, #@%#, #+js)
    // Look for first occurrence of # followed by another # or +
    let bytes = line.as_bytes();
    let mut sep_pos = None;
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'#' && (bytes[i + 1] == b'#' || bytes[i + 1] == b'+' || bytes[i + 1] == b'@' || bytes[i + 1] == b'?' || bytes[i + 1] == b'$' || bytes[i + 1] == b'%') {
            sep_pos = Some(i);
            break;
        }
    }

    let sep_pos = sep_pos?;
    let domain_part = &line[..sep_pos];

    // Check if domain part has ", " (comma followed by space)
    if !domain_part.contains(", ") {
        return None;
    }

    // Remove spaces after commas in domain part only
    let mut fixed_domains = String::with_capacity(domain_part.len());
    let mut chars = domain_part.chars().peekable();
    while let Some(ch) = chars.next() {
        fixed_domains.push(ch);
        if ch == ',' {
            while chars.peek() == Some(&' ') || chars.peek() == Some(&'\t') {
                chars.next();
            }
        }
    }

    let fixed = format!("{}{}", fixed_domains, &line[sep_pos..]);
    if fixed != line {
        Some(Typo {
            fixed,
            description: "Space after comma in domain list".to_string(),
        })
    } else {
        None
    }
}

/// Wrong cosmetic domain separator (using | instead of ,)
static WRONG_COSMETIC_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9~][a-zA-Z0-9\.\-,]*\.[a-zA-Z]{2,})\|([a-zA-Z0-9~][a-zA-Z0-9\.\-\|,]*)(#[@?$%]?#|#@[$%?]#|#\+js)").unwrap()
});

// =============================================================================
// Network Rule Typo Patterns
// =============================================================================

/// Triple $$$ before domain= ($$$domain= ? $domain=)
static TRIPLE_DOLLAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$\$domain=").unwrap());

/// Double $$ before domain= ($$domain= ? $domain=)
static DOUBLE_DOLLAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$domain=").unwrap());

/// Missing $ before domain= (after common file extensions)
static MISSING_DOLLAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\.(js|css|html|php|json|xml|gif|png|jpg|jpeg|svg|webp|woff2?|ttf|eot|mp[34]|m3u8)|\^)domain=([a-zA-Z0-9][\w\-]*\.[a-zA-Z]{2,})").unwrap()
});

/// Wrong domain separator (using , instead of |)
/// Lookahead ensures the token after the comma is also a domain (has a dot + TLD),
/// preventing false positives on option names like "image" or "script".
static WRONG_DOMAIN_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(domain=|\|)([a-zA-Z0-9~\*][a-zA-Z0-9\.\-\*]*\.[a-zA-Z]{2,}),([a-zA-Z0-9~\*][a-zA-Z0-9\.\-\*]*\.[a-zA-Z]{2,})")
        .unwrap()
});

// =============================================================================
// Typo Detection
// =============================================================================

#[derive(Debug, Clone)]
pub struct Typo {
    pub fixed: String,
    pub description: String,
}

/// Helper to create Typo if regex matches and changes line
#[inline]
fn try_fix(line: &str, pattern: &Regex, replacement: &str, description: &str) -> Option<Typo> {
    let fixed = pattern.replace_all(line, replacement);
    if fixed != line {
        return Some(Typo {
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
    if line.len() < 4 || line.starts_with('!') || line.starts_with('[') || line.starts_with('%') {
        return None;
    }

    // Network rules - check for $$ and $$$ typos
    if line.starts_with("||")
        || line.starts_with('|')
        || line.starts_with("@@")
        || line.contains("$domain=")
        || line.contains(",domain=")
    {
        // Use try_fix to avoid running the regex twice (is_match + replace)
        // Note: in regex replacements, "$$" is a literal "$"
        if let Some(typo) = try_fix(line, &TRIPLE_DOLLAR, "$$domain=", "Triple $ ($$$ ? $)") {
            return Some(typo);
        }
        if let Some(typo) = try_fix(line, &DOUBLE_DOLLAR, "$$domain=", "Double $ ($$ ? $)") {
            return Some(typo);
        }
        if let Some(typo) = try_fix(line, &MISSING_DOLLAR, "$1$$domain=$3", "Missing $ before domain=") {
            return Some(typo);
        }
        if let Some(typo) = try_fix(line, &WRONG_DOMAIN_SEPARATOR, "$1$2|$3", "Wrong domain separator (, ? |)") {
            return Some(typo);
        }

        return None; // No cosmetic typos in network rules
    }

    // Skip non-cosmetic rules (no # at all)
    if !line.contains('#') {
        return None;
    }

    // Check for wrong cosmetic domain separator (| instead of ,)
    if let Some(typo) = try_fix(
        line,
        &WRONG_COSMETIC_SEPARATOR,
        "$1,$2$3",
        "Wrong cosmetic separator (| ? ,)",
    ) {
        return Some(typo);
    }

    // Check for extra # (### ? ##)
    if let Some(caps) = EXTRA_HASH.captures(line) {
        let hashes = &caps[2];
        if hashes.len() > 2 {
            let fixed = EXTRA_HASH.replace(line, "${1}##${3}").to_string();
            return Some(Typo {
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
        .or_else(|| detect_space_after_comma(line))
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
            println!(
                "  {}:{}: {} ? {}",
                add.file, add.line_num, add.content, typo.fixed
            );
        } else {
            use owo_colors::OwoColorize;
            println!(
                "  {}:{}: {} ? {}",
                add.file.cyan(),
                add.line_num,
                add.content.red(),
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
    fn test_space_after_comma() {
        // Space after comma in domain list
        let typo = detect_typo("domain.com, domain2.com##.ad").unwrap();
        assert_eq!(typo.fixed, "domain.com,domain2.com##.ad");

        // Multiple spaces
        let typo = detect_typo("domain.com,  domain2.com##.ad").unwrap();
        assert_eq!(typo.fixed, "domain.com,domain2.com##.ad");

        // Multiple domains with spaces
        let (fixed, _) = fix_all_typos("a.com, b.com, c.com##.ad");
        assert_eq!(fixed, "a.com,b.com,c.com##.ad");

        // With +js
        let typo = detect_typo("domain.com, domain2.com##+js(aopr)").unwrap();
        assert_eq!(typo.fixed, "domain.com,domain2.com##+js(aopr)");

        // No space should not match
        assert!(detect_typo("domain.com,domain2.com##.ad").is_none());

        // Spaces inside selector must NOT be touched
        assert!(detect_typo("domain.com##+js(set-cookie, cookieAcknowledged, true)").is_none());
        assert!(detect_typo("domain.com##body:has-text(hello, world)").is_none());
        assert!(detect_typo("##.ad:has(.banner, .popup)").is_none());

        // Real-world case: domain list + js with spaces in args
        let (fixed, fixes) = fix_all_typos("domain.com, stromnetz.berlin##+js(set-cookie, cookieAgree, true)");
        assert_eq!(fixed, "domain.com,stromnetz.berlin##+js(set-cookie, cookieAgree, true)");
        assert_eq!(fixes.len(), 1);
    }

    #[test]
    fn test_triple_dollar() {
        let result = detect_typo("@@||example.com/cc.js$$$domain=asket.com");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().fixed,
            "@@||example.com/cc.js$domain=asket.com"
        );
    }

    #[test]
    fn test_double_dollar() {
        let result = detect_typo("@@||example.com/cc.js$$domain=asket.com");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().fixed,
            "@@||example.com/cc.js$domain=asket.com"
        );

        let result = detect_typo("||example.com/ad.js$$domain=test.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap().fixed, "||example.com/ad.js$domain=test.com");
    }

    #[test]
    fn test_missing_dollar() {
        let result = detect_typo("@@||example.com/cc.jsdomain=asket.com");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().fixed,
            "@@||example.com/cc.js$domain=asket.com"
        );

        // With ^ separator
        let result = detect_typo("@@||example.com/cc.js^domain=asket.com");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().fixed,
            "@@||example.com/cc.js^$domain=asket.com"
        );

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
        assert_eq!(
            result.unwrap().fixed,
            "||example.com$domain=site1.com|site2.com"
        );

        // Multiple commas (fix_all_typos handles iteratively)
        let (fixed, fixes) = fix_all_typos("||example.com$3p,domain=a.com,b.com,c.com");
        assert_eq!(fixed, "||example.com$3p,domain=a.com|b.com|c.com");
        assert_eq!(fixes.len(), 2);

        // Mixed separators
        let (fixed, _) = fix_all_typos("*.global/$3p,domain=animepahe.si,daddyhd.com|soap2day.day");
        assert_eq!(
            fixed,
            "*.global/$3p,domain=animepahe.si|daddyhd.com|soap2day.day"
        );

        // Valid pipe separator should not match
        let result = detect_typo("||example.com$domain=site1.com|site2.com");
        assert!(result.is_none());

        // Option name after domain should not be treated as domain separator typo
        let result = detect_typo("||example.com$domain=site1.com,image");
        assert!(result.is_none());

        let result = detect_typo("||example.com$image,domain=site1.com");
        assert!(result.is_none());
    }
}
