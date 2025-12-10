//! FOP Tests
//! Consolidated tests for main.rs and fop_sort.rs
//!
//! Copyright (C) 2025 FanboyNZ (FOP Rust)
//! https://github.com/ryanbr/fop-rs
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

use crate::{
    valid_url, check_comment,
    TLD_ONLY_PATTERN, LOCALHOST_PATTERN,
};

use crate::fop_sort::{
    convert_ubo_options, sort_domains, remove_unnecessary_wildcards, filter_tidy,
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
fn test_tld_only_pattern() {
    assert!(TLD_ONLY_PATTERN.is_match("||.org^"));
    assert!(TLD_ONLY_PATTERN.is_match(".com"));
    assert!(TLD_ONLY_PATTERN.is_match("|.net^"));
    assert!(!TLD_ONLY_PATTERN.is_match("||example.org^"));
}

#[test]
fn test_check_comment() {
    assert!(check_comment("M: Fixed typo", false));
    assert!(check_comment("A: (filters) https://example.com/issue", true));
    assert!(!check_comment("Invalid comment", false));
    assert!(!check_comment("A: (filters) not-a-url", true));
}

#[test]
fn test_localhost_pattern() {
    // Test 0.0.0.0 entries
    assert!(LOCALHOST_PATTERN.is_match("0.0.0.0 domain.com"));
    assert!(LOCALHOST_PATTERN.is_match("0.0.0.0 sub.domain.com"));
    assert!(LOCALHOST_PATTERN.is_match("0.0.0.0 ads.example.org"));
    
    // Test 127.0.0.1 entries
    assert!(LOCALHOST_PATTERN.is_match("127.0.0.1 domain.com"));
    assert!(LOCALHOST_PATTERN.is_match("127.0.0.1 sub.domain.com"));
    assert!(LOCALHOST_PATTERN.is_match("127.0.0.1 tracker.net"));
    
    // Test non-matching entries
    assert!(!LOCALHOST_PATTERN.is_match("# comment"));
    assert!(!LOCALHOST_PATTERN.is_match("192.168.1.1 domain.com"));
    assert!(!LOCALHOST_PATTERN.is_match("domain.com"));
}

#[test]
fn test_localhost_domain_extraction() {
    let caps = LOCALHOST_PATTERN.captures("0.0.0.0 z-ads.com").unwrap();
    assert_eq!(&caps[2], "z-ads.com");
    
    let caps = LOCALHOST_PATTERN.captures("127.0.0.1 sub.domain.com").unwrap();
    assert_eq!(&caps[2], "sub.domain.com");
    
    let caps = LOCALHOST_PATTERN.captures("0.0.0.0 a-tracker.net").unwrap();
    assert_eq!(&caps[2], "a-tracker.net");
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
fn test_sort_domains() {
    let mut domains = vec!["z.com".to_string(), "a.com".to_string(), "~b.com".to_string()];
    sort_domains(&mut domains);
    assert_eq!(domains, vec!["a.com", "~b.com", "z.com"]);
}
