//! Typo detection and correction for cosmetic filter rules
//!
//! Common typos:
//! - ###.class ? ##.class
//! - ##..class ? ##.class
//! - domain#.class ? domain##.class
//! - domain,,domain##.ad ? domain,domain##.ad

use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

// =============================================================================
// Cosmetic Typo Patterns
// =============================================================================

/// Cosmetic rule with extra # (###.class or domain###.class)
static EXTRA_HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^#]*)(###+)([.#\[\*].*)").unwrap());

/// Single # that should be ## (domain#.class)
static SINGLE_HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^#]+)#([.#\[\*\+][a-zA-Z])").unwrap());

/// Double dot in cosmetic selector (##..class)
static DOUBLE_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(##)\.\.([a-zA-Z])").unwrap());

/// Two or more separators in a row in a network pattern (`||example.com^^`)
static DOUBLE_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^\^+").unwrap());

/// Double comma in domain list (domain,,domain)
static DOUBLE_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",,+").unwrap());

/// Trailing comma before ## (domain,##.ad)
static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",+(#[@?$%]?#)").unwrap());

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
    Some(Typo {
        fixed,
        description: Cow::Borrowed("Space after comma in domain list"),
    })
}

/// Wrong cosmetic domain separator (using | instead of ,)
static WRONG_COSMETIC_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9~][a-zA-Z0-9\.\-,]*\.[a-zA-Z]{2,})\|([a-zA-Z0-9~][a-zA-Z0-9\.\-\|,]*)(#[@?$%]?#|#@[$%?]#|#\+js)").unwrap()
});

// =============================================================================
// Network Rule Typo Patterns
// =============================================================================

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
    pub description: Cow<'static, str>,
}

/// Helper to create Typo if regex matches and changes line
#[inline]
fn try_fix(line: &str, pattern: &Regex, replacement: &str, description: &'static str) -> Option<Typo> {
    match pattern.replace_all(line, replacement) {
        Cow::Owned(fixed) => Some(Typo {
            fixed,
            description: Cow::Borrowed(description),
        }),
        Cow::Borrowed(_) => None,
    }
}

/// Fix literal substring typos without regex.
#[inline]
fn try_fix_literal(line: &str, needle: &str, replacement: &str, description: &'static str) -> Option<Typo> {
    if !line.contains(needle) {
        return None;
    }
    Some(Typo {
        fixed: line.replacen(needle, replacement, 1),
        description: Cow::Borrowed(description),
    })
}

/// Strip leading commas without regex.
#[inline]
fn fix_leading_comma(line: &str) -> Option<Typo> {
    if !line.starts_with(',') {
        return None;
    }
    let trimmed = line.trim_start_matches(',');
    if trimmed.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
        Some(Typo {
            fixed: trimmed.to_string(),
            description: Cow::Borrowed("Leading comma removed"),
        })
    } else {
        None
    }
}

/// Check a cosmetic rule for typos
#[inline]
/// `^^` in a network rule's pattern.
///
/// `^` stands for one separator character, or the end of the address, so a
/// second one asks for a separator that was not meant to be required:
/// `||id5-tech.io^^` misses `https://id5-tech.io/ads.js`, where `^` alone
/// matches. It is legal syntax rather than a parse error, which is why nothing
/// rejects it -- the rule simply blocks less than it reads as blocking.
///
/// Only the pattern is looked at. An option carries regexes of its own --
/// `$replace=`, `$header=`, `$uritransform=` -- where `^^` can be deliberate,
/// and in a regex pattern `^` is an anchor, not a separator, so both are left
/// alone.
fn fix_double_separator(line: &str) -> Option<Typo> {
    // First, because it rejects every line but a handful and the rest of this
    // is wasted on them: the pattern is a slice of the line, so no `^^` in the
    // line means none in the pattern either. That spares every other line the
    // search for the options and the probe for a regex.
    //
    // A `memchr` scan over the carets was tried in its place, on the thought
    // that a two-byte needle pays for a Two-Way searcher: over the easylist
    // tree the two measure the same, 0.0417s against 0.0419s, so the plainer
    // one stands. Nor does the check as a whole show up -- 0.0427s with no
    // check at all, which is the same figure again.
    if !line.contains("^^") {
        return None;
    }
    let prefix = if line.starts_with("@@") { 2 } else { 0 };
    let body = &line[prefix..];
    // Judged before the options are split off, because a regex may hold a `$`
    // of its own -- `/ads\^\^$/$script` -- and splitting on the first one
    // would cut the pattern in half and leave what looks like a typo behind.
    // Slash-delimited is what makes a pattern a regex, so a path like
    // `/ads/banner^^` is still a pattern and still gets fixed.
    if body.starts_with('/') && (body.ends_with('/') || body.contains("/$")) {
        return None;
    }
    let pattern_len = prefix + body.find('$').unwrap_or(body.len());
    let pattern = &line[..pattern_len];
    if !pattern.contains("^^") {
        return None;
    }
    // What is left has to look like a URL pattern, because `^` means nothing
    // anywhere else and rewriting it there is corruption, not a fix.
    //
    // A pattern holds no whitespace -- a space would be `%20` -- so anything
    // with one is something else. `^` is not legal in a host name, so no valid
    // hosts entry carries `^^` at all; but `hosts_entry_split` validates only
    // the address and keeps whatever follows the space as the host, so a
    // malformed line like `0.0.0.0 weird^^host.example` is read as an entry
    // and kept as written everywhere -- and this was the one path rewriting
    // it.
    //
    // And a URL pattern carries a `.` or a `/`. Without one there is no
    // address in it, and a line of prose that the tidying has already run its
    // spaces out of is not a rule to correct.
    if pattern.bytes().any(|b| b.is_ascii_whitespace())
        || !pattern.bytes().any(|b| b == b'.' || b == b'/')
    {
        return None;
    }
    Some(Typo {
        fixed: format!(
            "{}{}",
            DOUBLE_SEPARATOR.replace_all(pattern, "^"),
            &line[pattern_len..]
        ),
        description: Cow::Borrowed("Double separator (^^ -> ^)"),
    })
}

pub fn detect_typo(line: &str) -> Option<Typo> {
    // Skip comments, empty lines, special directives, short lines
    if line.len() < 4 || line.starts_with('!') || line.starts_with('[') || line.starts_with('%') {
        return None;
    }

    // Fast reject: no trigger characters means no possible typo. `^` is here
    // for `^^`, which a rule can carry with none of the others -- though in
    // practice a network pattern has a `|` as well.
    if !line.bytes().any(|b| b == b'#' || b == b'$' || b == b',' || b == b'|' || b == b'^') {
        return None;
    }

    // Network rules - check for $$ and $$$ typos
    if line.starts_with("||")
        || line.starts_with('|')
        || line.starts_with("@@")
        || line.contains("$domain=")
        || line.contains(",domain=")
    {
        // Check for $ typos (literal match) then regex-based checks
        if let Some(typo) = try_fix_literal(line, "$$$domain=", "$domain=", "Triple $ ($$$ ? $)") {
            return Some(typo);
        }
        if let Some(typo) = try_fix_literal(line, "$$domain=", "$domain=", "Double $ ($$ ? $)") {
            return Some(typo);
        }
        if let Some(typo) = try_fix(line, &MISSING_DOLLAR, "$1$$domain=$3", "Missing $ before domain=") {
            return Some(typo);
        }
        if let Some(typo) = try_fix(line, &WRONG_DOMAIN_SEPARATOR, "$1$2|$3", "Wrong domain separator (, ? |)") {
            return Some(typo);
        }
        if let Some(typo) = fix_double_separator(line) {
            return Some(typo);
        }

        return None; // No cosmetic typos in network rules
    }

    // No `#`, so nothing cosmetic to look for -- but a bare substring pattern
    // such as `id5-tech.io^^` never reaches the network branch above, and its
    // separators are worth the same look.
    if !line.contains('#') {
        return fix_double_separator(line);
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
            let fixed = format!("{}##{}", &caps[1], &caps[3]);
            return Some(Typo {
                fixed,
                description: Cow::Owned(format!("Extra # ({} ? ##)", hashes)),
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
        .or_else(|| fix_leading_comma(line))
        .or_else(|| detect_space_after_comma(line))
}

/// Fix all typos in a line (iterates until no more fixes)
pub fn fix_all_typos(line: &str) -> (String, Vec<String>) {
    let mut all_fixes = Vec::new();

    // Fast path: no typo on first check - return without allocating
    let Some(first) = detect_typo(line) else {
        return (line.to_string(), all_fixes);
    };
    all_fixes.push(first.description.into_owned());
    let mut current = first.fixed;

    // Limit iterations to prevent infinite loops
    for _ in 0..9 {
        let Some(typo) = detect_typo(&current) else { break };
        all_fixes.push(typo.description.into_owned());
        current = typo.fixed;
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
pub fn check_additions(additions: &[Addition]) -> Vec<(&Addition, Typo)> {
    additions
        .iter()
        .filter_map(|add| detect_typo(&add.content).map(|typo| (add, typo)))
        .collect()
}

/// Report typos in additions (formatted output)
pub fn report_addition_typos(typos: &[(&Addition, Typo)], no_color: bool) {
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

        let typo = detect_typo("domain#+js(aopr)").unwrap();
        assert_eq!(typo.fixed, "domain##+js(aopr)");
    }

    #[test]
    fn test_double_separator() {
        // `^` stands for one separator or the end of the address, so the second
        // asks for one more than was meant: `||id5-tech.io^^` does not match
        // `https://id5-tech.io/ads.js`, which `^` alone does.
        for (line, want) in [
            ("||id5-tech.io^^", "||id5-tech.io^"),
            ("||example.com^^$third-party", "||example.com^$third-party"),
            ("@@||allow.com^^", "@@||allow.com^"),
            // No `||`, so it never reaches the network checks by that door.
            ("id5-tech.io^^", "id5-tech.io^"),
            // A path is a pattern, not a regex, slash or no slash.
            ("/ads/banner^^$script", "/ads/banner^$script"),
            // More than two collapse to one.
            ("||example.com^^^", "||example.com^"),
            ("||example.com^^/path^^", "||example.com^/path^"),
        ] {
            let typo = detect_typo(line).unwrap_or_else(|| panic!("missed {}", line));
            assert_eq!(typo.fixed, want, "{}", line);
            assert_eq!(typo.description, "Double separator (^^ -> ^)");
            // Fixing is a fixed point: the result carries no typo of its own.
            assert!(detect_typo(want).is_none(), "{} was fixed twice", want);
        }

        // Left exactly as written: in a regex `^` is an anchor, an option
        // carries regexes of its own, and a cosmetic selector is not a pattern.
        for line in [
            // Literal `^^` inside a slash-delimited pattern: two anchors in a
            // regex, which is the author's to write. Escaped as `\^\^` there
            // is no `^^` to find at all, so these are the cases that prove the
            // regex is left alone.
            "/^^ads/$script",
            "@@/^^ads/",
            "/ads^^/",
            r"/^https?:\/\/ads\^\^/$script",
            // The regex holds a `$`, so splitting on the first one would cut
            // the pattern in half and leave what looks like a typo behind.
            r"/^https?:\/\/ads\^\^$/$script",
            r"@@/^ads\^\^/$script",
            r"||x.com^$replace=/a\^\^/b/",
            r"||x.com^$header=etag:/\^\^/",
            r#"example.com##[href^="/ad"]"#,
            "||example.com^",
            "||example.com^$third-party",
            // Hosts entries, which this check has no business in. A valid
            // one cannot carry `^^` at all, since `^` is not legal in a host
            // name -- these are here to say what the check is not for.
            "0.0.0.0 host.example",
            "127.0.0.1 ads.example.com",
            "::1 ip6-localhost",
            // And the malformed ones, which are what the guard actually
            // governs: `hosts_entry_split` validates only the address and
            // keeps whatever follows the space, so FOP reads these as entries
            // and leaves them as written -- bar this check, before the guard.
            "0.0.0.0 weird^^host.example",
            "127.0.0.1 ads^^.example.com",
            "::1 ip6^^-localhost",
            // No address in it at all, so not a rule to correct.
            "someprosewith^^caretsinit",
            "^^",
        ] {
            assert!(detect_typo(line).is_none(), "touched {}", line);
        }
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
