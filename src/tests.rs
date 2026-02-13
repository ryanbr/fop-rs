//! FOP Tests
//! Consolidated tests for main.rs and fop_sort.rs
//!
//! Copyright (C) 2025 FanboyNZ (FOP Rust)
//! https://github.com/ryanbr/fop-rs
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

use crate::fop_git::{check_comment, valid_url};
use crate::fop_sort::is_tld_only;

use crate::fop_sort::{
    convert_ubo_options, filter_tidy, is_localhost_entry, localhost_domain,
    remove_unnecessary_wildcards, sort_domains,
};

// =============================================================================
// Main.rs Tests
// =============================================================================

#[test]
fn test_valid_url() {
    assert!(valid_url("https://example.com/issue"));
    assert!(valid_url("http://example.com"));
    assert!(valid_url("about:blank"));
    assert!(!valid_url("not-a-url"));
    assert!(!valid_url("example.com")); // Missing scheme
}

#[test]
fn test_tld_only() {
    assert!(is_tld_only("||.org^"));
    assert!(is_tld_only(".com"));
    assert!(is_tld_only("|.net^"));
    assert!(!is_tld_only("||example.org^"));
}

#[test]
fn test_check_comment() {
    assert!(check_comment("M: Fixed typo", false));
    assert!(check_comment(
        "A: (filters) https://example.com/issue",
        true
    ));
    assert!(!check_comment("Invalid comment", false));
    assert!(!check_comment("A: (filters) not-a-url", true));
}

// =============================================================================
// Localhost Entry Tests
// =============================================================================

#[test]
fn test_is_localhost_entry() {
    // Valid 0.0.0.0 entries
    assert!(is_localhost_entry("0.0.0.0 domain.com"));
    assert!(is_localhost_entry("0.0.0.0 sub.domain.com"));
    assert!(is_localhost_entry("0.0.0.0 ads.example.org"));
    assert!(is_localhost_entry("0.0.0.0\tdomain.com"));

    // Valid 127.0.0.1 entries
    assert!(is_localhost_entry("127.0.0.1 domain.com"));
    assert!(is_localhost_entry("127.0.0.1 sub.domain.com"));
    assert!(is_localhost_entry("127.0.0.1 tracker.net"));

    // Invalid entries
    assert!(!is_localhost_entry("# comment"));
    assert!(!is_localhost_entry("192.168.1.1 domain.com"));
    assert!(!is_localhost_entry("domain.com"));
    assert!(!is_localhost_entry("0.0.0.0"));
    assert!(!is_localhost_entry("0.0.0.0 "));
    assert!(!is_localhost_entry("127.0.0.1"));
    assert!(!is_localhost_entry(""));
}

#[test]
fn test_localhost_domain_extraction() {
    assert_eq!(localhost_domain("0.0.0.0 z-ads.com"), "z-ads.com");
    assert_eq!(localhost_domain("127.0.0.1 sub.domain.com"), "sub.domain.com");
    assert_eq!(localhost_domain("0.0.0.0 a-tracker.net"), "a-tracker.net");
    assert_eq!(localhost_domain("0.0.0.0\tdomain.com"), "domain.com");
    // Multiple spaces
    assert_eq!(localhost_domain("0.0.0.0   spaced.com"), "spaced.com");
    // Fallback for non-localhost lines
    assert_eq!(localhost_domain("plain.domain.com"), "plain.domain.com");
}

#[test]
fn test_localhost_domain_sort_order() {
    let mut entries = vec![
        "0.0.0.0 z-tracker.com".to_string(),
        "0.0.0.0 a-ads.net".to_string(),
        "127.0.0.1 m-stats.org".to_string(),
    ];
    entries.sort_by_cached_key(|s| localhost_domain(s).to_ascii_lowercase());
    assert_eq!(entries[0], "0.0.0.0 a-ads.net");
    assert_eq!(entries[1], "127.0.0.1 m-stats.org");
    assert_eq!(entries[2], "0.0.0.0 z-tracker.com");
}

// =============================================================================
// fop_sort.rs Tests
// =============================================================================

#[test]
fn test_remove_unnecessary_wildcards() {
    assert_eq!(remove_unnecessary_wildcards("*example*"), "example");
    assert_eq!(remove_unnecessary_wildcards("**example**"), "example");
    assert_eq!(remove_unnecessary_wildcards("@@*example*"), "@@example");
    assert_eq!(remove_unnecessary_wildcards("*|example"), "*|example");
    assert_eq!(remove_unnecessary_wildcards("example|*"), "example|*");
    // Fast path: no wildcards
    assert_eq!(remove_unnecessary_wildcards("example.com"), "example.com");
    assert_eq!(remove_unnecessary_wildcards("@@||example.com^"), "@@||example.com^");
}

#[test]
fn test_convert_ubo_options() {
    let input = vec!["xhr".to_string(), "3p".to_string(), "frame".to_string()];
    let expected = vec!["xmlhttprequest", "third-party", "subdocument"];
    let result = convert_ubo_options(input);
    assert_eq!(result, expected);

    let input2 = vec!["from=example.com".to_string()];
    let result2 = convert_ubo_options(input2);
    assert_eq!(result2, vec!["domain=example.com"]);
}

#[test]
fn test_filter_tidy() {
    // Test option sorting
    let result = filter_tidy("||example.com^$image,script,third-party", true);
    assert!(result.contains("image"));
    assert!(result.contains("script"));
    assert!(result.contains("third-party"));

    // Test domain sorting
    let result = filter_tidy("||ad.com^$domain=z.com|a.com|m.com", true);
    assert!(result.contains("domain=a.com|m.com|z.com"));
}

#[test]
fn test_filter_tidy_no_options() {
    // Fast path: no $ in filter
    let result = filter_tidy("||example.com^", true);
    assert_eq!(result, "||example.com^");
}

#[test]
fn test_filter_tidy_regex_preserved() {
    // Regex value options should be preserved
    let result = filter_tidy("||example.com$removeparam=/regex/", true);
    assert!(result.contains("removeparam=/regex/"));
}

#[test]
fn test_filter_tidy_space_removal() {
    // Spaces should be removed from network filters
    let result = filter_tidy("|| example .com ^$script", true);
    assert_eq!(result, "||example.com^$script");
}

#[test]
fn test_filter_tidy_space_preserved_element() {
    // Spaces in element rules should be preserved
    let result = filter_tidy("example.com##div .ad", true);
    assert_eq!(result, "example.com##div .ad");
}

#[test]
fn test_filter_tidy_adguard_js_no_space_strip() {
    // #%# rules should not have spaces stripped
    let result = filter_tidy("example.com#%#(()=>{ console.log('test') })();", true);
    assert!(result.contains(" "), "#%# rule spaces should be preserved, got: {}", result);
}

#[test]
fn test_sort_domains() {
    let mut domains = vec![
        "z.com".to_string(),
        "a.com".to_string(),
        "~b.com".to_string(),
    ];
    sort_domains(&mut domains);
    assert_eq!(domains, vec!["a.com", "~b.com", "z.com"]);
}

// =============================================================================
// Attribute Selector Tests (preserve ~=)
// =============================================================================

#[test]
fn test_attribute_selector_tilde_equals_preserved() {
    use crate::fop_sort::element_tidy;
    
    // ~= means "attribute contains word" - should NOT add spaces
    let result = element_tidy("lowendtalk.com", "##", "#Panel a[rel~=\"sponsored\"]");
    assert!(result.contains("[rel~=\"sponsored\"]"), "~= should be preserved, got: {}", result);
}

#[test]
fn test_attribute_selector_alt_tilde() {
    use crate::fop_sort::element_tidy;
    
    let result = element_tidy("example.com", "##", "div[alt~=\"Ad\"]");
    assert!(result.contains("[alt~=\"Ad\"]"), "~= should be preserved, got: {}", result);
}

// =============================================================================
// *:not() and *:has() Preservation Tests
// =============================================================================

#[test]
fn test_star_not_preserved() {
    use crate::fop_sort::element_tidy;
    
    let result = element_tidy("em.com.br", "##", "div > * > *:not(.comment-header)");
    assert!(result.contains("*:not("), "* before :not() should be preserved, got: {}", result);
}

#[test]
fn test_star_has_preserved() {
    use crate::fop_sort::element_tidy;
    
    let result = element_tidy("example.com", "##", "div > *:has(.ad)");
    assert!(result.contains("*:has("), "* before :has() should be preserved, got: {}", result);
}

// =============================================================================
// Extended Syntax Preservation Tests
// =============================================================================

#[test]
fn test_has_with_attribute_selectors() {
    use crate::fop_sort::element_tidy;
    
    // :has() with attribute selectors inside - should be preserved exactly
    let result = element_tidy("tripadvisor.com", "##", "div:has(> div[class=\"ui_columns is-multiline \"])");
    assert!(result.contains(":has("), "Extended :has() should be preserved, got: {}", result);
    assert!(result.contains("[class=\"ui_columns is-multiline \"]"), "Attribute value should be preserved, got: {}", result);
}

#[test]
fn test_abp_extended_selectors() {
    use crate::fop_sort::element_tidy;
    
    // :-abp-contains should be preserved
    let result = element_tidy("kijiji.ca", "#?#", "[data-testid^=\"listing-card-list-item-\"]:-abp-contains(TOP AD)");
    assert!(result.contains(":-abp-contains("), ":-abp-contains should be preserved, got: {}", result);
}

#[test]
fn test_escaped_tailwind_classes() {
    use crate::fop_sort::element_tidy;
    
    // Escaped brackets and colons in Tailwind-style classes
    let result = element_tidy("theepochtimes.com", "##", ".bg-\\[\\#f8f8f8\\]");
    assert!(result.contains("\\["), "Escaped brackets should be preserved, got: {}", result);
}

// =============================================================================
// CSS Combinator and Attribute Selector Tests
// =============================================================================

#[test]
fn test_adjacent_sibling_with_universal() {
    use crate::fop_sort::element_tidy;
    
    // + * should be preserved (adjacent sibling with universal selector)
    let result = element_tidy("filecrypt.cc,filecrypt.co", "##", ".hghspd + *");
    assert!(result.contains("+ *"), "Adjacent sibling + * should be preserved, got: {}", result);
}

#[test]
fn test_attribute_selectors() {
    use crate::fop_sort::element_tidy;
    
    // Various attribute selector types
    let result = element_tidy("example.com", "##", "[class$=\"-ad\"]");
    assert!(result.contains("[class$=\"-ad\"]"), "Attribute ends-with should be preserved, got: {}", result);
    
    let result = element_tidy("example.com", "##", "[class*=\"-ad-\"]");
    assert!(result.contains("[class*=\"-ad-\"]"), "Attribute contains should be preserved, got: {}", result);
}

#[test]
fn test_complex_has_selectors() {
    use crate::fop_sort::element_tidy;
    
    // Complex :has() with nested attribute selectors
    let result = element_tidy("twitter.com,x.com", "##", "div[data-testid=\"cellInnerDiv\"] > div > div[class] > div[class][data-testid=\"placementTracking\"]");
    assert!(result.contains("[data-testid=\"placementTracking\"]"), "Complex attribute selector should be preserved, got: {}", result);
}

#[test]
fn test_has_with_href_contains() {
    use crate::fop_sort::element_tidy;
    
    // :has() with href contains
    let result = element_tidy("wayfair.com", "##", "div[data-hb-id=\"Grid.Item\"]:has(a[href*=\"&sponsoredid=\"])");
    assert!(result.contains(":has("), ":has() should be preserved, got: {}", result);
    assert!(result.contains("[href*=\"&sponsoredid=\"]"), "href contains should be preserved, got: {}", result);
}

// =============================================================================
// AdGuard Extended Syntax Tests
// =============================================================================

#[test]
fn test_adguard_js_injection_preserved() {
    use crate::fop_sort::element_tidy;

    // #%# JS injection - selector preserved, domains sorted
    let result = element_tidy("z.com,a.com", "#%#", "//scriptlet('prevent-window-open')");
    assert!(result.starts_with("a.com,z.com#%#"), "Domains should be sorted, got: {}", result);
    assert!(result.contains("//scriptlet('prevent-window-open')"), "Selector should be preserved, got: {}", result);
}

#[test]
fn test_adguard_js_injection_braces_preserved() {
    use crate::fop_sort::element_tidy;

    // #%# with JS braces - should be preserved exactly
    let result = element_tidy("example.com", "#%#", "(()=>{ window.test = true; })();");
    assert!(result.contains("(()=>{ window.test = true; })();"), "JS braces should be preserved, got: {}", result);
}

#[test]
fn test_adguard_css_injection_preserved() {
    use crate::fop_sort::element_tidy;

    // #$# CSS injection - selector preserved
    let result = element_tidy("example.com", "#$#", ".ad { display: none !important; }");
    assert!(result.contains("{ display: none !important; }"), "CSS injection should be preserved, got: {}", result);
}

#[test]
fn test_adguard_extended_css_preserved() {
    use crate::fop_sort::element_tidy;

    // #$?# extended CSS injection - selector preserved
    let result = element_tidy("z.com,a.com", "#$?#", "div[style*=\"position: fixed\"] { remove: true; }");
    assert!(result.starts_with("a.com,z.com#$?#"), "Domains should be sorted, got: {}", result);
    assert!(result.contains("{ remove: true; }"), "Extended CSS should be preserved, got: {}", result);
}

#[test]
fn test_adguard_html_filtering_preserved() {
    use crate::fop_sort::element_tidy;

    // $$ HTML filtering - selector preserved, domains sorted
    let result = element_tidy("z.com,a.com", "$$", "script[tag-content=\"adConfig\"]");
    assert!(result.starts_with("a.com,z.com$$"), "Domains should be sorted, got: {}", result);
    assert!(result.contains("script[tag-content=\"adConfig\"]"), "Selector should be preserved, got: {}", result);
}

#[test]
fn test_adguard_html_filtering_complex() {
    use crate::fop_sort::element_tidy;

    // $$ with wildcard and min/max-length
    let result = element_tidy("site.com", "$$", "script[wildcard=\"*function*break;case*\"][min-length=\"25000\"][max-length=\"100000\"]");
    assert!(result.contains("[wildcard="), "Wildcard attr should be preserved, got: {}", result);
    assert!(result.contains("[min-length="), "min-length should be preserved, got: {}", result);
}

#[test]
fn test_adguard_exception_separators() {
    use crate::fop_sort::element_tidy;

    // Exception variants
    let result = element_tidy("example.com", "#@$#", ".ad { display: none; }");
    assert!(result.contains("#@$#"), "Exception separator should be preserved, got: {}", result);

    let result = element_tidy("example.com", "#@%#", "//scriptlet('test')");
    assert!(result.contains("#@%#"), "Exception separator should be preserved, got: {}", result);

    let result = element_tidy("example.com", "#@$?#", ".ad { remove: true; }");
    assert!(result.contains("#@$?#"), "Exception separator should be preserved, got: {}", result);

    let result = element_tidy("example.com", "$@$", "script[tag-content=\"ad\"]");
    assert!(result.contains("$@$"), "Exception separator should be preserved, got: {}", result);
}

// =============================================================================
// :has-text() Merging Tests
// =============================================================================

#[test]
fn test_has_text_merge_two_plain() {
    let input = vec![
        "example.com##.ad:has-text(Buy now)".to_string(),
        "example.com##.ad:has-text(Subscribe)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains(":has-text(/Buy now|Subscribe/)") 
         || result[0].contains(":has-text(/Subscribe|Buy now/)"));
}

#[test]
fn test_has_text_merge_regex_and_plain() {
    let input = vec![
        "example.com##.ad:has-text(/regex pattern/)".to_string(),
        "example.com##.ad:has-text(plain text)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains(":has-text(/"));
    assert!(result[0].contains("regex pattern"));
    assert!(result[0].contains("plain text"));
}

#[test]
fn test_has_text_escape_special_chars() {
    let input = vec![
        "example.com##.price:has-text($9.99)".to_string(),
        "example.com##.price:has-text(50% off)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("\\$9\\.99"));
}

#[test]
fn test_has_text_different_base_selectors_not_merged() {
    let input = vec![
        "example.com##.ad:has-text(text1)".to_string(),
        "example.com##.banner:has-text(text2)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_has_text_different_domains_not_merged() {
    let input = vec![
        "example.com##.ad:has-text(text1)".to_string(),
        "other.com##.ad:has-text(text2)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_has_text_single_rule_unchanged() {
    let input = vec![
        "example.com##.ad:has-text(single)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "example.com##.ad:has-text(single)");
}

#[test]
fn test_has_text_abp_contains_merged() {
    let input = vec![
        "example.com##.ad:-abp-contains(text1)".to_string(),
        "example.com##.ad:-abp-contains(text2)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains(":-abp-contains(/text1|text2/)") 
         || result[0].contains(":-abp-contains(/text2|text1/)"));
}

#[test]
fn test_has_text_google_promo_example() {
    let input = vec![
        "www.google.com##[aria-describedby=\"promo_desc_id\"]:has-text(/Time for a new laptop|Gemini/)".to_string(),
        "www.google.com##[aria-describedby=\"promo_desc_id\"]:has-text(Keep things dark)".to_string(),
    ];
    let result = crate::fop_sort::combine_has_text_rules(input);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("www.google.com##"));
    assert!(result[0].contains("Time for a new laptop|Gemini"));
    assert!(result[0].contains("Keep things dark"));
}
