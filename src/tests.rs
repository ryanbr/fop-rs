//! FOP Tests
//! Consolidated tests for main.rs and fop_sort.rs
//!
//! Copyright (C) 2025 FanboyNZ (FOP Rust)
//! https://github.com/ryanbr/fop-rs
//!
//! Copyright (C) 2011 Michael (original Python version)
//! Rust port maintains GPL-3.0 license compatibility.

use crate::fop_git::{apply_commit_url_template, check_comment, default_template_for_base, mask_urls_in_message, mask_urls_in_message_ext, valid_url};
use crate::fop_sort::is_tld_only;
use crate::fop_datestamp::is_version_line;

use crate::fop_sort::{
    convert_ubo_options, filter_tidy, is_localhost_entry, localhost_domain,
    malformed_rule_reason, remove_unnecessary_wildcards, sort_domains,
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

#[test]
fn test_mask_urls_in_message() {
    // Level 1: [.]
    assert_eq!(
        mask_urls_in_message("A: https://www.example.com/foo", 1, false),
        "A: https://www[.]example[.]com/foo"
    );
    // Level 2: (.)
    assert_eq!(
        mask_urls_in_message("A: https://www.example.com/foo", 2, false),
        "A: https://www(.)example(.)com/foo"
    );
    // Level 3: space
    assert_eq!(
        mask_urls_in_message("A: https://www.example.com/foo", 3, false),
        "A: https://www example com/foo"
    );
    // Unknown level → fallback to level 1 ([.])
    assert_eq!(
        mask_urls_in_message("A: https://www.example.com/foo", 9, false),
        "A: https://www[.]example[.]com/foo"
    );
    assert_eq!(
        mask_urls_in_message("A: https://www.example.com/foo", 0, false),
        "A: https://www[.]example[.]com/foo"
    );
    // No URL → unchanged
    assert_eq!(
        mask_urls_in_message("M: Update filters", 1, false),
        "M: Update filters"
    );
    // Multiple URLs masked, prose dots untouched
    let out = mask_urls_in_message(
        "M: see https://a.com and https://b.io. thanks.",
        1,
        false,
    );
    assert_eq!(out, "M: see https://a[.]com and https://b[.]io. thanks.");
    // Only host dots are masked; path/query dots are left alone
    assert_eq!(
        mask_urls_in_message(
            "A: https://media-amazon.com/images/S/sash/$domain=imdb.com",
            1,
            false,
        ),
        "A: https://media-amazon[.]com/images/S/sash/$domain=imdb.com"
    );
    // Path with file extension stays clickable
    assert_eq!(
        mask_urls_in_message("A: https://example.org/foo/bar.html", 1, false),
        "A: https://example[.]org/foo/bar.html"
    );
    // Query string dots untouched
    assert_eq!(
        mask_urls_in_message("A: https://forums.lanik.us/viewtopic.php?t=1.2", 1, false),
        "A: https://forums[.]lanik[.]us/viewtopic.php?t=1.2"
    );
    // Level 4: preserve all subdomain dots, mask only within eTLD+1
    assert_eq!(
        mask_urls_in_message("A: https://www.example.com/foo", 4, false),
        "A: https://www.example[.]com/foo"
    );
    assert_eq!(
        mask_urls_in_message("A: https://www.example.co.nz/foo", 4, false),
        "A: https://www.example[.]co[.]nz/foo"
    );
    // Level 4 with apex (no subdomain) — defang the only dot
    assert_eq!(
        mask_urls_in_message("A: https://example.com/foo", 4, false),
        "A: https://example[.]com/foo"
    );
    // Level 4 apex with compound TLD — mask all dots inside eTLD+1
    assert_eq!(
        mask_urls_in_message("A: https://example.co.nz/foo", 4, false),
        "A: https://example[.]co[.]nz/foo"
    );
    // Level 4 with deep subdomain chain — all subdomain dots preserved
    assert_eq!(
        mask_urls_in_message("A: https://a.b.c.example.com/foo", 4, false),
        "A: https://a.b.c.example[.]com/foo"
    );
    // Level 4 with deep subdomain + compound TLD
    assert_eq!(
        mask_urls_in_message("A: https://a.b.example.co.uk/foo", 4, false),
        "A: https://a.b.example[.]co[.]uk/foo"
    );
    // Level 4 with Nigerian compound TLD
    assert_eq!(
        mask_urls_in_message("A: https://www.seriezloaded.com.ng/foo", 4, false),
        "A: https://www.seriezloaded[.]com[.]ng/foo"
    );
    // Level 4 with Hong Kong compound TLD
    assert_eq!(
        mask_urls_in_message("A: https://news.example.com.hk/foo", 4, false),
        "A: https://news.example[.]com[.]hk/foo"
    );
    // Level 5: Unicode ONE DOT LEADER (U+2024) replaces host dots
    assert_eq!(
        mask_urls_in_message("A: https://www.example.com/foo", 5, false),
        "A: https://www\u{2024}example\u{2024}com/foo"
    );
    // Level 5 leaves path/query dots alone, like other levels
    assert_eq!(
        mask_urls_in_message("A: https://forums.lanik.us/t.php?x=1.2", 5, false),
        "A: https://forums\u{2024}lanik\u{2024}us/t.php?x=1.2"
    );
    // Without mask_bare: bare domains are NOT masked
    assert_eq!(
        mask_urls_in_message("A: see example.com for details", 1, false),
        "A: see example.com for details"
    );
    // GitHub URLs are exempt (clickability preserved)
    assert_eq!(
        mask_urls_in_message("A: https://github.com/easylist/easylist/issues/123", 1, false),
        "A: https://github.com/easylist/easylist/issues/123"
    );
    // GitHub subdomains are also exempt
    assert_eq!(
        mask_urls_in_message("A: https://gist.github.com/foo/abc", 1, false),
        "A: https://gist.github.com/foo/abc"
    );
    // GitLab apex and subdomains exempt
    assert_eq!(
        mask_urls_in_message("A: https://gitlab.com/foo/bar/-/issues/1", 1, false),
        "A: https://gitlab.com/foo/bar/-/issues/1"
    );
    assert_eq!(
        mask_urls_in_message("A: https://docs.gitlab.com/ee/", 1, false),
        "A: https://docs.gitlab.com/ee/"
    );
    // Lookalike that ends in gitlab.com but isn't a subdomain still gets masked
    assert_eq!(
        mask_urls_in_message("A: https://notgitlab.com/foo", 1, false),
        "A: https://notgitlab[.]com/foo"
    );
    // Codeberg apex and subdomains exempt
    assert_eq!(
        mask_urls_in_message("A: https://codeberg.org/foo/bar/issues/1", 1, false),
        "A: https://codeberg.org/foo/bar/issues/1"
    );
    assert_eq!(
        mask_urls_in_message("A: https://docs.codeberg.org/", 1, false),
        "A: https://docs.codeberg.org/"
    );
    // Non-github URL still gets masked, github stays untouched
    assert_eq!(
        mask_urls_in_message(
            "M: see https://github.com/easylist/easylist/issues/1 and https://forums.lanik.us/t=2",
            1,
            false,
        ),
        "M: see https://github.com/easylist/easylist/issues/1 and https://forums[.]lanik[.]us/t=2"
    );

    // ---- Bare-domain masking (mask_bare = true) ----
    // Bare hostname gets masked at level 1
    assert_eq!(
        mask_urls_in_message("A: see example.com for details", 1, true),
        "A: see example[.]com for details"
    );
    // Bare hostname respects level 4 (preserve subdomain dot)
    assert_eq!(
        mask_urls_in_message("A: see www.example.com for details", 4, true),
        "A: see www.example[.]com for details"
    );
    // Bare github.com still exempt
    assert_eq!(
        mask_urls_in_message("M: see github.com/foo/bar", 1, true),
        "M: see github.com/foo/bar"
    );
    // Version numbers like 1.2.3 do NOT match (final label is digits)
    assert_eq!(
        mask_urls_in_message("M: bumped to 1.2.3 today", 1, true),
        "M: bumped to 1.2.3 today"
    );
    // Filenames CAN match (acceptable false positive — opt-in)
    assert_eq!(
        mask_urls_in_message("M: see config.toml", 1, true),
        "M: see config[.]toml"
    );
    // Scheme URL still wins when both forms present
    assert_eq!(
        mask_urls_in_message("A: https://forums.lanik.us/foo", 1, true),
        "A: https://forums[.]lanik[.]us/foo"
    );

    // ---- Custom exempt hosts via mask_urls_in_message_ext ----
    let extras = vec!["git.company.internal".to_string(), "gitea.example.org".to_string()];
    // Self-hosted apex exempt
    assert_eq!(
        mask_urls_in_message_ext("A: https://git.company.internal/foo/issues/1", 1, false, &extras),
        "A: https://git.company.internal/foo/issues/1"
    );
    // Subdomain of self-hosted apex also exempt
    assert_eq!(
        mask_urls_in_message_ext("A: https://wiki.gitea.example.org/page", 1, false, &extras),
        "A: https://wiki.gitea.example.org/page"
    );
    // Non-exempt third-party still masked
    assert_eq!(
        mask_urls_in_message_ext("A: https://forums.lanik.us/foo", 1, false, &extras),
        "A: https://forums[.]lanik[.]us/foo"
    );
    // Lookalike that ends in extra apex but isn't a subdomain still gets masked (level 1 → all dots)
    assert_eq!(
        mask_urls_in_message_ext("A: https://notgitea.example.org/foo", 1, false, &extras),
        "A: https://notgitea[.]example[.]org/foo"
    );
}

#[test]
fn test_apply_commit_url_template() {
    let base = "https://github.com/foo/bar";
    let sha = "abc12345";

    // Default github-style
    assert_eq!(
        apply_commit_url_template("{base}/commit/{sha}", base, sha),
        "https://github.com/foo/bar/commit/abc12345"
    );
    // Bitbucket-style (plural)
    assert_eq!(
        apply_commit_url_template("{base}/commits/{sha}", "https://bitbucket.org/foo/bar", sha),
        "https://bitbucket.org/foo/bar/commits/abc12345"
    );
    // GitLab canonical form
    assert_eq!(
        apply_commit_url_template("{base}/-/commit/{sha}", "https://gitlab.com/foo/bar", sha),
        "https://gitlab.com/foo/bar/-/commit/abc12345"
    );
    // Custom path segments preserved verbatim
    assert_eq!(
        apply_commit_url_template("{base}/r/{sha}/inspect", base, sha),
        "https://github.com/foo/bar/r/abc12345/inspect"
    );
    // Unknown placeholders stay literal
    assert_eq!(
        apply_commit_url_template("{base}/commit/{shA}", base, sha),
        "https://github.com/foo/bar/commit/{shA}"
    );
    // Multiple instances of {sha} both substituted
    assert_eq!(
        apply_commit_url_template("{base}/c/{sha}?q={sha}", base, sha),
        "https://github.com/foo/bar/c/abc12345?q=abc12345"
    );
}

#[test]
fn test_default_template_for_base() {
    // github / gitlab / gitea / codeberg / forgejo / sourcehut all use /commit/
    assert_eq!(default_template_for_base("https://github.com/foo/bar"), "{base}/commit/{sha}");
    assert_eq!(default_template_for_base("https://gitlab.com/foo/bar"), "{base}/commit/{sha}");
    assert_eq!(default_template_for_base("https://codeberg.org/foo/bar"), "{base}/commit/{sha}");
    assert_eq!(default_template_for_base("https://gitea.example.org/foo/bar"), "{base}/commit/{sha}");
    // Bitbucket uses /commits/ (plural)
    assert_eq!(default_template_for_base("https://bitbucket.org/foo/bar"), "{base}/commits/{sha}");
    // Bitbucket subdomain
    assert_eq!(default_template_for_base("https://api.bitbucket.org/foo/bar"), "{base}/commits/{sha}");
    // Lookalike that ends in bitbucket.org but isn't a subdomain stays on default
    assert_eq!(default_template_for_base("https://notbitbucket.org/foo/bar"), "{base}/commit/{sha}");
    // Self-hosted unknown — defaults to github-style
    assert_eq!(default_template_for_base("https://git.company.internal/foo/bar"), "{base}/commit/{sha}");
    // Non-ASCII (IDN) remote host must not panic on a byte-boundary slice.
    // This host is 18 bytes, so the old `host[len - 13..]` landed at byte 5 —
    // inside a 3-byte character.
    assert_eq!(default_template_for_base("https://\u{4f8b}\u{4f8b}\u{4f8b}\u{4f8b}\u{4f8b}.jp/foo/bar"), "{base}/commit/{sha}");
    assert_eq!(default_template_for_base("https://\u{65e5}\u{672c}\u{8a9e}.example.jp/foo/bar"), "{base}/commit/{sha}");
    // Host shorter than "bitbucket.org" but non-ASCII
    assert_eq!(default_template_for_base("https://\u{e4}.de/foo/bar"), "{base}/commit/{sha}");
    // Port and userinfo must be stripped before the host check, as
    // is_excluded_host does — otherwise these miss and get the wrong template.
    assert_eq!(default_template_for_base("https://bitbucket.org:443/u/r"), "{base}/commits/{sha}");
    assert_eq!(default_template_for_base("ssh://git@bitbucket.org/u/r"), "{base}/commits/{sha}");
    assert_eq!(default_template_for_base("ssh://git@api.bitbucket.org:22/u/r"), "{base}/commits/{sha}");
    // ...and stripping them must not turn a non-Bitbucket host into one.
    assert_eq!(default_template_for_base("https://github.com:443/u/r"), "{base}/commit/{sha}");
    assert_eq!(default_template_for_base("ssh://git@github.com/u/r"), "{base}/commit/{sha}");
    assert_eq!(default_template_for_base("https://notbitbucket.org:443/u/r"), "{base}/commit/{sha}");
}

#[test]
fn test_filter_tidy_adguard_exception_forms_preserved() {
    // `$@$` and `#@?#` were missing from the element-rule check, so these had
    // their selector whitespace stripped while the identical `$$` / `#?#`
    // forms were left alone.
    for rule in [
        "example.com$@$script[tag-content=\"ad config\"]",
        "example.com$$script[tag-content=\"ad config\"]",
        "example.com#@?#div.ad { x: y; }",
        "example.com#?#div.ad { x: y; }",
        "example.com$@$script[wildcard=\"*function break*\"]",
    ] {
        assert_eq!(filter_tidy(rule, false), rule, "must survive intact: {}", rule);
    }
}

#[test]
fn test_filter_tidy_network_rule_with_hash_or_dollars_in_path() {
    // The cosmetic check that gates the `$option.option` fix is anchored, so a
    // network rule whose URL path happens to contain `##` or `$$` is still a
    // network rule and still gets its options normalised. A substring test
    // read these as cosmetic and skipped the fix.
    assert_eq!(
        filter_tidy("||a.com/$$p^$third-party.script", false),
        "||a.com/$$p^$script,third-party"
    );
    assert_eq!(
        filter_tidy("||a.com/a##b^$third-party.script", false),
        "||a.com/a##b^$script,third-party"
    );
    // A genuine cosmetic rule is still exempt.
    assert_eq!(
        filter_tidy("example.com##div[data-x=\"a.b$c.d\"]", false),
        "example.com##div[data-x=\"a.b$c.d\"]"
    );

    // Regex-domain cosmetic rules too. ADGUARD_ELEMENT_PATTERN's domain group
    // rejects a leading `/`, so these match only REGEX_ELEMENT_PATTERN -- and
    // their host anchor ends in `$`, which reads as an option separator, so
    // testing the AdGuard pattern alone let the typo fix corrupt them.
    for rule in [
        r"/^\w+\.example\.com$/##.ad",
        r"/ads\.example\.com$/#@#.ad",
        r"/^ad\d+\.com$/##div.banner",
        r"/regex/##.a$b.c",
        r"/^x\d+$/#?#div.ad",
    ] {
        assert_eq!(filter_tidy(rule, false), rule, "regex-domain rule must survive: {}", rule);
    }

    // But a regex *network* pattern with no cosmetic separator is still a
    // network rule and still gets its options normalised.
    assert_eq!(
        filter_tidy(r"/ads\.js$/$third-party.script", false),
        r"/ads\.js$/$script,third-party"
    );
}

#[test]
fn test_filter_tidy_all_wildcard_rule_not_blanked() {
    // A rule that is nothing but wildcards reduces to `*`, which
    // remove_unnecessary_wildcards clears — it was written back as a blank
    // line. Every path through filter_tidy must put the `*` back.
    assert_eq!(filter_tidy("***", false), "*");
    assert_eq!(filter_tidy("**", false), "*");
    assert_eq!(filter_tidy("*", false), "*");
    assert_eq!(filter_tidy("@@***", false), "@@*");
    assert!(!filter_tidy("***", false).is_empty(), "must never produce a blank line");
}

#[test]
fn test_filter_tidy_cosmetic_dollar_not_option_separator() {
    // The `$` inside a cosmetic separator is not an option separator. Reading
    // it as one made the selector and stylesheet body look like an option list,
    // and the `$option.option` typo fix rewrote every `.` in it to `,` --
    // turning `div.ad` ("div with class ad") into `div,ad` ("every div or every
    // ad element"). Silent: the result is still a valid rule.
    for rule in [
        "example.com#$#div.ad { display: none; }",
        "example.com#@$#div.ad { display: none; }",
        "example.com#$?#div.ad { x: y; }",
        "example.com#@$?#div.ad { x: y; }",
        "example.com#$?#div:has(> .ad) { x: y; }",
        "example.com$$div.ad",
        "example.com$@$div.ad",
        "#$#body.cls { color: red; }",
        "example.com#$?#div:contains(a.b) { x: y; }",
        // Not a `$` rule at all, but the same hazard: a literal `$` inside a
        // plain cosmetic selector must not start an option list either.
        "example.com##div[data-x=\"a.b$c.d\"]",
    ] {
        assert_eq!(filter_tidy(rule, false), rule, "must pass through unchanged: {}", rule);
    }

    // The typo fix it guards still works on real network options.
    assert_eq!(
        filter_tidy("||a.com^$third-party.script", false),
        "||a.com^$script,third-party"
    );
    assert_eq!(
        filter_tidy("||b.com^$image.script.font", false),
        "||b.com^$font,image,script"
    );
    // An escaped `\$` in a value is still not an option separator.
    assert_eq!(
        filter_tidy("||a.com^$removeparam=/^\\$ja=/", false),
        "||a.com^$removeparam=/^\\$ja=/"
    );
}

#[test]
fn test_malformed_rule_reason() {
    // Debris from a truncated selector — the case the check exists for.
    assert_eq!(malformed_rule_reason("\"])", false), Some("invalid start"));
    assert_eq!(malformed_rule_reason("])", false), Some("invalid start"));
    assert_eq!(malformed_rule_reason("}]", false), Some("invalid start"));
    assert_eq!(malformed_rule_reason(")foo", false), Some("invalid start"));

    // Valid 3-character rules — these were deleted by the old 4-char floor.
    assert_eq!(malformed_rule_reason("##a", false), None);
    assert_eq!(malformed_rule_reason("*/*", false), None);
    assert_eq!(malformed_rule_reason("/a/", false), None);

    // Leading characters an allowlist kept missing (274 `_` rules, 15 `=`,
    // 8 `%` and 2 `^` across the local lists) must survive.
    assert_eq!(malformed_rule_reason("_ad_banner", false), None);
    assert_eq!(malformed_rule_reason("%2Fads", false), None);
    assert_eq!(malformed_rule_reason("^tracker^", false), None);
    assert_eq!(malformed_rule_reason("=ad_id", false), None);
    assert_eq!(malformed_rule_reason("||example.com^", false), None);
    assert_eq!(malformed_rule_reason("##.ad", false), None);

    // Genuinely too short.
    assert_eq!(malformed_rule_reason("ab", false), Some("too short"));
    assert_eq!(malformed_rule_reason("#", false), Some("too short"));
    // Total on empty input: the call site guarantees non-empty, but the
    // helper must not index into an empty slice regardless.
    assert_eq!(malformed_rule_reason("", false), Some("too short"));

    // Length is counted in characters, not bytes. A lone 3-byte character is
    // one character and is not a rule; a byte floor would have kept it.
    assert_eq!(malformed_rule_reason("\u{65e5}", false), Some("too short"));
    assert_eq!(malformed_rule_reason("\u{65e5}\u{672c}", false), Some("too short"));
    assert_eq!(malformed_rule_reason("\u{65e5}\u{672c}\u{8a9e}", false), None);
    // A stray UTF-8 BOM is 3 bytes but one character — not a rule.
    assert_eq!(malformed_rule_reason("\u{feff}", false), Some("too short"));
    // 4-byte characters count as one too.
    assert_eq!(malformed_rule_reason("\u{1f600}", false), Some("too short"));
}

#[test]
fn test_is_version_line() {
    assert!(is_version_line("! Version: 202601011200"));
    assert!(is_version_line("!Version: 1"));
    assert!(is_version_line("# version: 1"));
    assert!(!is_version_line("! Title: EasyList"));
    // Non-ASCII comments must not panic on the byte-8 slice.
    assert!(!is_version_line("! \u{65e5}\u{672c}\u{8a9e}\u{3067}\u{3059}"));
    assert!(!is_version_line("! \u{421}\u{43f}\u{438}\u{441}\u{43e}\u{43a}"));
    assert!(!is_version_line("! \u{1f600}\u{1f600}\u{1f600}"));
}

#[test]
fn test_remote_url_to_https_forms() {
    use crate::fop_git::remote_url_to_https as f;
    // SCP form (already handled).
    assert_eq!(f("git@github.com:u/r.git"), "https://github.com/u/r");
    // ssh:// form was passed through untouched, so the printed commit URL was
    // `ssh://git@host/u/r/commits/<sha>` — not a link.
    assert_eq!(f("ssh://git@bitbucket.org/u/r.git"), "https://bitbucket.org/u/r");
    assert_eq!(f("ssh://git@api.bitbucket.org:22/u/r"), "https://api.bitbucket.org/u/r");
    // Credentials in an https remote must never reach the printed URL.
    assert_eq!(f("https://x-access-token:SECRET@github.com/u/r.git"), "https://github.com/u/r");
    assert_eq!(f("https://user@github.com/u/r"), "https://github.com/u/r");
    // Plain https is unchanged apart from the .git suffix.
    assert_eq!(f("https://github.com/u/r.git"), "https://github.com/u/r");
    assert_eq!(f("https://github.com/u/r"), "https://github.com/u/r");
    // http:// leaks the same credentials — a self-hosted host on plain http is
    // exactly where an embedded CI token lives. The scheme is preserved.
    assert_eq!(f("http://x-access-token:SECRET@gitea.internal/u/r.git"), "http://gitea.internal/u/r");
    assert_eq!(f("http://gitea.internal/u/r"), "http://gitea.internal/u/r");
    // A password may contain '@': splitting at the first one left a fragment of
    // it behind and named a host that does not exist.
    assert_eq!(f("https://user:p@ss@github.com/u/r"), "https://github.com/u/r");
    // An '@' in the path is not userinfo.
    assert_eq!(f("https://github.com/u/r@v2"), "https://github.com/u/r@v2");
    // scp form with a deploy-key user, not just `git@`.
    assert_eq!(f("deploy@github.com:u/r.git"), "https://github.com/u/r");
    assert_eq!(f("github.com:u/r.git"), "https://github.com/u/r");
    // A Windows drive letter is not a host.
    assert_eq!(f("C:/repos/r"), "C:/repos/r");
    // A bare local path is left alone.
    assert_eq!(f("/srv/git/r.git"), "/srv/git/r");
    // Schemes that are not ssh/http(s) are not scp form: generalising the scp
    // branch past `git@` briefly turned these into `https://git///host/...`.
    assert_eq!(f("git://host.example/u/r.git"), "git://host.example/u/r");
    assert_eq!(f("file:///srv/git/r.git"), "file:///srv/git/r");
    // git+ssh is an ssh remote and can carry userinfo, so it normalises.
    assert_eq!(f("git+ssh://user:tok@host.example/u/r"), "https://host.example/u/r");
    // A password may contain '/' too — base64-derived tokens routinely do —
    // which made the authority appear to end before the userinfo did.
    assert_eq!(f("https://user:a/b@gitea.internal/u/r"), "https://gitea.internal/u/r");
    assert_eq!(f("https://x:a/b@c/d@gitea.internal/u/r.git"), "https://gitea.internal/u/r");
    // An authority-only remote still has its userinfo stripped.
    assert_eq!(f("https://user:tok@gitea.internal"), "https://gitea.internal");
    // ...but a versioned path is not userinfo, even when it looks host-ish.
    assert_eq!(f("https://github.com/u/r@v2"), "https://github.com/u/r@v2");
}

#[test]
fn test_generate_pr_url_shares_the_normaliser() {
    use crate::fop_git::generate_pr_url as f;
    // A CI remote's token must not reach the printed "Create PR at:" line.
    let url = f("https://x-access-token:SECRET@github.com/u/r.git", "main", "feat", None)
        .expect("github remote should yield a PR url");
    assert!(!url.contains("SECRET"), "token leaked into PR url: {}", url);
    assert_eq!(url, "https://github.com/u/r/compare/main...feat?expand=1");
    // Deploy-key and ssh:// remotes used to fall through to None.
    assert_eq!(
        f("deploy@github.com:u/r.git", "main", "feat", None).as_deref(),
        Some("https://github.com/u/r/compare/main...feat?expand=1")
    );
    assert_eq!(
        f("ssh://git@gitlab.com/u/r.git", "main", "feat", None).as_deref(),
        Some("https://gitlab.com/u/r/-/merge_requests/new?merge_request[source_branch]=feat&merge_request[target_branch]=main")
    );
    // A scheme with no web equivalent still declines.
    assert_eq!(f("git://host.example/u/r.git", "main", "feat", None), None);
}

#[test]
fn test_mask_exempt_host_with_userinfo() {
    // `git@github.com` is still github.com and must stay exempt — the host
    // extractor stripped the port but not userinfo.
    let msg = "A: https://git@github.com/easylist/easylist/issues/1";
    assert_eq!(mask_urls_in_message(msg, 1, false), msg);
    let msg2 = "A: https://github.com:443/easylist/easylist/issues/1";
    assert_eq!(mask_urls_in_message(msg2, 1, false), msg2);
    // A non-exempt host with userinfo is still masked.
    assert_eq!(
        mask_urls_in_message("A: https://git@example.com/x", 1, false),
        "A: https://git@example[.]com/x"
    );
}

#[test]
fn test_mask_level4_compound_tld_case() {
    // Level 4 masks every dot inside the eTLD+1 and preserves subdomain dots.
    // The compound-TLD lookup is case-insensitive, so an uppercase or mixed
    // -case TLD must be recognised as compound — otherwise only the final dot
    // is masked and the registrable domain stays joined by a real dot.
    assert_eq!(
        mask_urls_in_message("A: https://www.Example.CO.UK/foo", 4, false),
        "A: https://www.Example[.]CO[.]UK/foo"
    );
    assert_eq!(
        mask_urls_in_message("A: https://www.example.Co.Uk/foo", 4, false),
        "A: https://www.example[.]Co[.]Uk/foo"
    );
    assert_eq!(
        mask_urls_in_message("A: https://WWW.EXAMPLE.COM.AU/foo", 4, false),
        "A: https://WWW.EXAMPLE[.]COM[.]AU/foo"
    );
    // Lowercase behaviour is unchanged.
    assert_eq!(
        mask_urls_in_message("A: https://www.example.co.uk/foo", 4, false),
        "A: https://www.example[.]co[.]uk/foo"
    );
    // Apex of a compound TLD, uppercase — every dot is inside eTLD+1.
    assert_eq!(
        mask_urls_in_message("A: https://Example.CO.UK/foo", 4, false),
        "A: https://Example[.]CO[.]UK/foo"
    );
    // A single-label TLD stays single-label regardless of case.
    assert_eq!(
        mask_urls_in_message("A: https://www.Example.COM/foo", 4, false),
        "A: https://www.Example[.]COM/foo"
    );
    // Not a compound TLD despite looking like one.
    assert_eq!(
        mask_urls_in_message("A: https://www.example.ZZ.UK/foo", 4, false),
        "A: https://www.example.ZZ[.]UK/foo"
    );
}

#[test]
fn test_mask_urls_non_ascii_and_case() {
    // A non-ASCII (IDN) host must be masked, not panic on a byte-index slice.
    let out = mask_urls_in_message("A: https://\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}.\u{440}\u{444}/x", 1, false);
    assert_eq!(out, "A: https://\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}[.]\u{440}\u{444}/x");

    // An uppercase scheme is still a URL and must be masked.
    assert_eq!(
        mask_urls_in_message("A: HTTPS://Example.com/foo", 1, false),
        "A: HTTPS://Example[.]com/foo"
    );
    assert_eq!(
        mask_urls_in_message("A: HtTp://example.com/foo", 1, false),
        "A: HtTp://example[.]com/foo"
    );
    // Uppercase scheme on an exempt host stays exempt.
    assert_eq!(
        mask_urls_in_message("A: HTTPS://GitHub.com/foo/bar", 1, false),
        "A: HTTPS://GitHub.com/foo/bar"
    );

    // Bare mode: Unicode case folding must not make `[a-z]` match U+212A
    // (KELVIN SIGN) or U+017F, which previously produced a match starting
    // mid-character and panicked on `trimmed[..7]`.
    let kelvin = "M: see abcdef\u{212a}.com now";
    let out = mask_urls_in_message(kelvin, 1, true);
    assert_eq!(out, kelvin, "U+212A must not be treated as an ASCII letter");
    let long_s = "M: see abcdef\u{17f}.com now";
    assert_eq!(mask_urls_in_message(long_s, 1, true), long_s);
    // ...but a genuine uppercase bare host still masks in bare mode.
    assert_eq!(
        mask_urls_in_message("M: see Example.COM now", 1, true),
        "M: see Example[.]COM now"
    );
    // A non-ASCII host in bare mode must not panic either.
    let idn_bare = "M: see \u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}.\u{440}\u{444} now";
    assert_eq!(mask_urls_in_message(idn_bare, 1, true), idn_bare);

    // Exemption matching stays char-boundary safe for a non-ASCII extra host,
    // and matches when the bytes are identical.
    let msg = "A: https://\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}.\u{440}\u{444}/x";
    let exempt = vec!["\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}.\u{440}\u{444}".to_string()];
    assert_eq!(mask_urls_in_message_ext(msg, 1, false, &exempt), msg);

    // A case-variant of the same IDN host is exempt too: matching folds
    // Unicode case for non-ASCII hosts, so a configured exemption isn't
    // silently ignored just because the message spells the host differently.
    let upper = "A: https://\u{41f}\u{420}\u{418}\u{41c}\u{415}\u{420}.\u{420}\u{424}/x";
    assert_eq!(mask_urls_in_message_ext(upper, 1, false, &exempt), upper);
    // Subdomain of an IDN exempt host, mixed case.
    let sub = "A: https://WWW.\u{41f}\u{420}\u{418}\u{41c}\u{415}\u{420}.\u{420}\u{424}/x";
    assert_eq!(mask_urls_in_message_ext(sub, 1, false, &exempt), sub);
    // A different IDN host is still masked.
    let other = "A: https://\u{434}\u{440}\u{443}\u{433}\u{43e}\u{439}.\u{440}\u{444}/x";
    assert_eq!(
        mask_urls_in_message_ext(other, 1, false, &exempt),
        "A: https://\u{434}\u{440}\u{443}\u{433}\u{43e}\u{439}[.]\u{440}\u{444}/x"
    );
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
    let mut entries = [
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
    // `*` before `$` is the pattern of an options-only rule, not padding.
    // Reached directly when the options don't match OPTION_PATTERN.
    assert_eq!(
        remove_unnecessary_wildcards("*$csp=script-src 'self'"),
        "*$csp=script-src 'self'"
    );
    assert_eq!(
        remove_unnecessary_wildcards("**$csp=script-src 'self'"),
        "*$csp=script-src 'self'"
    );
}

#[test]
fn test_filter_tidy_keeps_authors_options_only_form() {
    // Both spellings of an options-only rule are valid. fop keeps whichever the
    // author wrote rather than normalising one into the other -- the lists hold
    // the bare form throughout, and rewriting them all would be pure churn.
    for bare in [
        "$ping,third-party",
        "$popup,domain=a.com",
        "$websocket,domain=a.com",
        "@@$document",
        "@@$generichide,domain=a.com",
        "$csp=script-src 'self'",
    ] {
        assert_eq!(filter_tidy(bare, false), bare, "bare form must stay bare: {}", bare);
    }
    // ...and the wildcard form keeps its wildcard.
    for starred in [
        "*$ping,third-party",
        "*$popup,domain=a.com",
        "@@*$document",
        "@@*$generichide,domain=a.com",
        "*$csp=script-src 'self'",
    ] {
        assert_eq!(filter_tidy(starred, false), starred, "wildcard must survive: {}", starred);
    }
}

#[test]
fn test_filter_tidy_keeps_wildcard_on_options_only_rules() {
    // A rule with options but no pattern is spelled `*$opts`. The `*` is the
    // pattern; dropping it yields the pattern-less `$opts` form, which not
    // every consumer of these lists accepts.
    for rule in [
        "*$ping,third-party",
        "*$third-party",
        "*$ping,domain=a.com",
        "*$popup,third-party,domain=a.com|b.com",
        "@@*$ping,third-party",
        "@@*$generichide,domain=a.com",
        "@@*$document",
        "*$csp=script-src 'self'",
        "*$replace=/a/b/",
    ] {
        assert_eq!(filter_tidy(rule, false), rule, "wildcard must survive: {}", rule);
    }

    // A repeated wildcard collapses to one — the repeat is just untidy.
    assert_eq!(filter_tidy("**$ping", false), "*$ping");
    assert_eq!(filter_tidy("***$ping,third-party", false), "*$ping,third-party");
    assert_eq!(filter_tidy("@@**$document", false), "@@*$document");

    // Rules that do have a pattern are still tidied as before.
    assert_eq!(filter_tidy("*ads*", false), "ads");
    assert_eq!(filter_tidy("@@*ads*", false), "@@ads");
    assert_eq!(filter_tidy("*ads*$third-party", false), "ads$third-party");
    // Existing guards are untouched.
    assert_eq!(filter_tidy("*|$ping", false), "*|$ping");
    assert_eq!(filter_tidy("*||a.com^", false), "*||a.com^");
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
fn test_filter_tidy_jsonprune_no_commas() {
    // jsonprune with dot-separated path — dots preserved, spaces preserved
    let result = filter_tidy(
        "||assets.msn.com/service/news/feed/pages/$jsonprune=\\$.sections..subSections..cards..[?(key-substr 'type' 'nativead')]",
        true,
    );
    assert!(result.contains("jsonprune=\\$.sections..subSections..cards.."), "jsonprune value broken: {}", result);
    assert!(result.contains("key-substr 'type' 'nativead'"), "jsonprune spaces stripped: {}", result);
}

#[test]
fn test_filter_tidy_jsonprune_with_domain() {
    // jsonprune followed by domain= option — comma should separate them
    let result = filter_tidy(
        ".com/playlist?list=$jsonprune=\\$.playerConfig.ssapConfig,domain=youtubekids.com|youtube-nocookie.com|youtube.com",
        true,
    );
    assert!(result.contains("jsonprune=\\$.playerConfig.ssapConfig"), "jsonprune value broken: {}", result);
    assert!(result.contains("domain=youtube-nocookie.com|youtube.com|youtubekids.com"), "domain missing or unsorted: {}", result);
}

#[test]
fn test_filter_tidy_jsonprune_escaped_commas() {
    // jsonprune with escaped commas in value, followed by domain=
    let result = filter_tidy(
        ".com/watch?$xmlhttprequest,jsonprune=\\$..[adPlacements\\, adSlots\\, playerAds],domain=youtubekids.com|youtube-nocookie.com|youtube.com",
        true,
    );
    assert!(result.contains("jsonprune=\\$..[adPlacements\\, adSlots\\, playerAds]"), "jsonprune escaped commas broken: {}", result);
    assert!(result.contains("domain="), "domain option lost: {}", result);
    assert!(result.contains("xmlhttprequest"), "xmlhttprequest option lost: {}", result);
}

#[test]
fn test_filter_tidy_jsonprune_complex_path() {
    // jsonprune with complex JSON path, no other options
    let result = filter_tidy(
        "||msn.com/resolver/api/resolve/$jsonprune=\\$.configs[\"ConsumptionPage/gallery_default\"].properties.componentConfigs.slideshowConfigs..interstitialNativeAds",
        true,
    );
    assert!(result.contains("jsonprune=\\$.configs[\"ConsumptionPage/gallery_default\"]"), "jsonprune complex path broken: {}", result);
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

#[test]
fn test_sort_domains_with_ancestor_marker() {
    // The >> ancestor-context marker should keep the domain grouped with its base
    let mut domains = vec![
        "z.com".to_string(),
        "example.com>>".to_string(),
        "a.com".to_string(),
        "example.com".to_string(),
    ];
    sort_domains(&mut domains);
    assert_eq!(domains, vec!["a.com", "example.com", "example.com>>", "z.com"]);
}

#[test]
fn test_filter_tidy_ancestor_marker() {
    // Rule with >> suffix should be preserved
    let result = filter_tidy("tomsguide.com>>##+js(trusted-click-element, button)", true);
    assert!(result.contains("tomsguide.com>>"), "Ancestor marker lost: {}", result);
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

#[test]
fn test_convert_selectors_separators_belong_to_adguard() {
    use crate::fop_sort::convert_selectors as f;
    const ABP_HIDE: &str = "example.com##.ad:-abp-contains(Anzeige)";
    const ABP_EXC: &str = "example.com#@#.ad:-abp-contains(Anzeige)";
    // --abp-convert renames operators only; uBO reads ## and #@# for these.
    assert_eq!(f(ABP_HIDE, true, false), "example.com##.ad:has-text(Anzeige)");
    assert_eq!(f(ABP_EXC, true, false), "example.com#@#.ad:has-text(Anzeige)");
    // --adguard-convert promotes both separators to AdGuard's spelling.
    assert_eq!(f(ABP_HIDE, true, true), "example.com#?#.ad:has-text(Anzeige)");
    assert_eq!(f(ABP_EXC, true, true), "example.com#@?#.ad:has-text(Anzeige)");
    // It is independent: a rule with nothing for --abp-convert to convert is
    // still promoted, and no operator is renamed.
    let ubo_hide = "racurs.ua##.c:has(> .h:has-text(/новини|новости/i))";
    let ubo_exc = "racurs.ua#@#.c:has(> .h:has-text(/новини|новости/i))";
    assert_eq!(f(ubo_hide, false, true), "racurs.ua#?#.c:has(> .h:has-text(/новини|новости/i))");
    assert_eq!(f(ubo_exc, false, true), "racurs.ua#@?#.c:has(> .h:has-text(/новини|новости/i))");
    // ...but the promotion keys on `:has-text()`, so a rule still in ABP
    // operator form is left alone until --abp-convert rewrites it.
    assert_eq!(f(ABP_HIDE, false, true), ABP_HIDE);
    assert_eq!(f(ABP_EXC, false, true), ABP_EXC);
    // Neither flag is a no-op conversion.
    assert_eq!(f(ABP_EXC, false, false), ABP_EXC);
    assert_eq!(f(ubo_exc, true, false), ubo_exc);

    for abp in [true, false] {
        // A rule that already carries the AdGuard spelling is never touched —
        // this is the form the region allowlists ship.
        assert_eq!(
            f("wintersport.nl#@?#.mb-6:contains(gesponsord)", abp, true),
            "wintersport.nl#@?#.mb-6:contains(gesponsord)"
        );
        // HTML filtering is uBO-specific and keeps ##^.
        assert_eq!(
            f("example.com##^script:has-text(ads)", abp, true),
            "example.com##^script:has-text(ads)"
        );
        // A non-element rule is untouched.
        assert_eq!(f("||example.com/ads^", abp, true), "||example.com/ads^");
    }
    // :has() alone is native CSS — plain ## is right even under --adguard-convert.
    assert_eq!(
        f("example.com##.ad:-abp-has(.x)", true, true),
        "example.com##.ad:has(.x)"
    );
    assert_eq!(
        f("berliner-zeitung.de#@?#.m-article-teaser:-abp-has(div:-abp-contains(Anzeige))", true, true),
        "berliner-zeitung.de#@?#.m-article-teaser:has(div:has-text(Anzeige))"
    );
}

#[test]
fn test_ignore_line_minimum_keeps_short_rules() {
    use crate::fop_sort::malformed_rule_reason as f;
    // Default: one- and two-character rules are dropped as malformed.
    for short in ["#", "ab", "*", "/a", ""] {
        assert_eq!(f(short, false), Some("too short"), "{:?}", short);
        assert_eq!(f(short, true), None, "{:?} should survive the flag", short);
    }
    // The flag only lifts the length floor -- debris is still debris, however
    // long, so a truncated selector tail is dropped either way.
    for debris in ["\"])", "])", "}]", ")foo"] {
        assert_eq!(f(debris, false), Some("invalid start"), "{:?}", debris);
        assert_eq!(f(debris, true), Some("invalid start"), "{:?}", debris);
    }
    // Three characters is already valid, so the flag changes nothing there.
    for ok in ["##a", "*/*", "/a/", "||example.com^"] {
        assert_eq!(f(ok, false), None);
        assert_eq!(f(ok, true), None);
    }
}

#[test]
fn test_check_rule_flags_bad_additions() {
    use crate::fop_rules::check_rule as f;
    // Incomplete cosmetic rules.
    assert_eq!(f("example.com##").unwrap().reason, "separator with no selector");
    assert_eq!(f("example.com#?#").unwrap().reason, "separator with no selector");
    assert_eq!(f("example.com#@#").unwrap().reason, "separator with no selector");
    // Truncated selectors, which the leading-character check cannot see.
    assert_eq!(
        f("example.com##.ad[href=\"x\"").unwrap().reason,
        "unbalanced brackets in selector"
    );
    assert_eq!(f("example.com##)").unwrap().reason, "unbalanced brackets in selector");
    // Note the gap this leaves: `:has-text(` takes literal arguments, so an
    // unterminated one cannot be told from a `)` that is part of the text.
    assert!(f("example.com##.ad:has-text(x").is_none());
    assert_eq!(f("example.com##.ad{color:red").unwrap().reason, "unbalanced brackets in selector");
    // Incomplete option lists.
    assert_eq!(f("||example.com$").unwrap().reason, "option marker with no options");
    assert_eq!(f("||example.com$domain=").unwrap().reason, "option with no value");
    assert_eq!(f("||example.com$third-party,").unwrap().reason, "empty option");
    // A malformed option list fails the sorter's pattern the same way a line
    // that is not a rule does, so these two are only told apart behind an
    // unambiguous anchor. Without one the checker stays quiet, which is the
    // right side to err on: the loose splitter this replaced flagged
    // `$removeparam=/^utm$/` and shell `$PATH` as removable defects.
    assert!(f("notarule.txt$third-party,").is_none());
    // Misspelled options -- the common case for a hand-written rule.
    let p = f("||example.com$thrid-party").unwrap();
    assert_eq!((p.reason, p.detail), ("unknown option", "thrid-party"));
    assert_eq!(f("||example.com$domian=example.org").unwrap().reason, "unknown option");
}

#[test]
fn test_check_rule_leaves_valid_rules_alone() {
    use crate::fop_rules::check_rule as f;
    for ok in [
        // Brackets that are unbalanced only inside a string or a regex.
        "example.com##[href=\"(\"]",
        "example.com##.a:has-text(/\\)/)",
        "example.com##.ad[href^=\"http\"][data-x='a(b']",
        // Ordinary rules of each shape.
        "example.com##.ad",
        "example.com#@#.ad",
        "example.com#?#.ad:has-text(x)",
        "||example.com^$third-party",
        "||example.com^$domain=a.com|b.com,important",
        "||example.com^$removeparam=/^utm_/",
        "||example.com^$csp=script-src 'none'",
        "||example.com^$redirect=noopjs",
        "/ads/banner.gif",
        "||example.com/path#fragment",
        "@@||example.com^$~third-party",
        // Not ours to judge.
        "! comment",
        "[Adblock Plus 2.0]",
        "[$path=/x/]example.com##.ad",
        "127.0.0.1 example.com",
        "",
    ] {
        assert!(f(ok).is_none(), "{:?} flagged as {:?}", ok, f(ok).map(|p| p.reason));
    }
}

#[test]
fn test_check_rule_extra_shapes() {
    use crate::fop_rules::check_rule as f;
    // Braces balance like brackets -- AdGuard CSS injection relies on them.
    assert_eq!(f("example.com#$#.ad { color: red").unwrap().reason, "unbalanced brackets in selector");
    assert!(f("example.com#$#.ad { color: red; }").is_none());
    // Domain lists.
    assert_eq!(f(",example.com##.ad").unwrap().reason, "malformed domain list");
    assert_eq!(f("a.com,,b.com##.ad").unwrap().reason, "malformed domain list");
    assert_eq!(f("exa..mple.com##.ad").unwrap().reason, "malformed domain list");
    assert!(f("a.com,~b.com,example.*##.ad").is_none());
    // A regex domain keeps its dots and slashes.
    assert!(f(r"/^ad\d+\.example\..*/##.ad").is_none());
    // A selector cannot open on a combinator, but +js( is a scriptlet.
    assert_eq!(f("example.com##> div").unwrap().reason, "selector starts with a combinator");
    assert_eq!(f("example.com##+ div").unwrap().reason, "selector starts with a combinator");
    assert!(f("example.com##+js(aopr, x)").is_none());
    // Pipe-separated option values.
    assert_eq!(f("||x.com^$domain=a.com|").unwrap().reason, "empty entry in option value");
    assert_eq!(f("||x.com^$denyallow=|b.com").unwrap().reason, "empty entry in option value");
    assert!(f("||x.com^$domain=a.com|~b.com").is_none());
    // A regex option value keeps its pipes.
    assert!(f("||x.com^$domain=/a|b/").is_none());
    // Literal-argument constructs are exempt from balancing entirely.
    for literal in [
        "katestube.com##+js(nostif, '0x)",
        "sunporno.com##+js(aeld, , ;})",
        "t-online.de##^script:has-text(}(window);)",
        "hdfull.*##+js(aeld, mousedown, !!{});)",
    ] {
        assert!(f(literal).is_none(), "{:?} flagged", literal);
    }
}

#[test]
fn test_suggest_option_catches_any_misspelling() {
    use crate::suggest_option as s;
    // Generic, rather than a table of known typos: distance to the real set.
    for (typo, want) in [
        ("thrid-party", "third-party"),
        ("domian", "domain"),
        ("docuemnt", "document"),
        ("scrpit", "script"),
        ("removeparm", "removeparam"),
        ("redirct", "redirect"),
        ("elemhid", "elemhide"),
        ("genericblcok", "genericblock"),
        ("objectsubrequest", "object-subrequest"),
        ("matchcase", "match-case"),
    ] {
        assert_eq!(s(typo), Some(want), "{}", typo);
    }
    // An option that is simply new must not be "corrected" into something else.
    assert_eq!(s("totallynewoption2030"), None);
    assert_eq!(s("aaaaaaaaaaaaaaaa"), None);
    // Short names get a tighter budget, so unrelated three-letter options are
    // not proposed for one another.
    assert_eq!(s("zzz"), None);

    // End to end, the suggestion rides along with the problem.
    let p = crate::fop_rules::check_rule("||example.com^$thrid-party").unwrap();
    assert_eq!((p.reason, p.detail, p.suggestion), ("unknown option", "thrid-party", Some("third-party")));
    // ...and a rule whose options are all known carries none.
    assert!(crate::fop_rules::check_rule("||example.com^$third-party,domain=a.com").is_none());
}


#[test]
fn test_bare_domain_is_flagged_but_never_removed() {
    use crate::fop_rules::check_rule as f;
    for bare in ["domain.com", "anotherdomain.co.nz", "sub.example.org", "xn--80ak6aa92e.com", "a-b.io"] {
        let p = f(bare).expect(bare);
        assert_eq!(p.reason, "bare domain, did you mean ||host^ ?");
        // `removable` now only labels the report -- --remove-bad-rules takes
        // every flagged line, so what is left to commit is what passed. The
        // distinction still drives the CI exit code: advice does not fail a
        // build.
        assert!(!p.removable, "{} should be reported as advice", bare);
    }
    // `domain.com^` is no longer in this list: it carries filter syntax but is
    // still missing its anchor, so it is reported as such by its own check.
    assert!(f("domain.com^").unwrap().reason.contains("no || anchor"));
    assert!(f("domain.com$third-party").unwrap().reason.contains("no || anchor"));
    // Anything else carrying filter syntax is the author being explicit.
    for ok in [
        "||domain.com^", "|http://domain.com", "domain.com/path",
        "@@domain.com", "domain.com##.ad",
        // Substring patterns that merely look like hostnames.
        ".cookielaw.js", "_chartbeat.js", "adserver.gif", ".PrivacyDataNotice.",
        "ads.php", "track.json",
        // Not hostname-shaped.
        "nodots", "-lead.com", "trail-.com", "a..b.com", "x.toolongtobeatldxxxxxxxxxxxxxx",
        // Hosts-file entries carry a space.
        "127.0.0.1 domain.com",
    ] {
        assert!(f(ok).is_none(), "{:?} flagged as {:?}", ok, f(ok).map(|p| p.reason));
    }
}


#[test]
fn test_check_rule_ignores_non_rule_lines() {
    use crate::fop_rules::check_rule as f;
    // A `$` is not an option marker outside a filter rule. These reach the
    // checker whenever a commit touches a script, a workflow or source, and
    // were previously reported as removable defects.
    for not_a_rule in [
        "export PATH=$PATH:/usr/bin",
        "  run: echo \"$GITHUB_SHA\"",
        "let x = format!(\"{}\", $y);",
        "sed -i \"s/^version = .*/version = \\\"$V\\\"/\"",
        "some: $value",
    ] {
        assert!(f(not_a_rule).is_none(), "{:?} flagged as {:?}", not_a_rule, f(not_a_rule).map(|p| p.reason));
    }
    // Valid rules whose pattern or value legitimately contains `$` or a comma.
    for valid in [
        "||example.com^$removeparam=/^utm$/",
        "||example.com/script.js$replace=/(foo)bar/$1baz/",
        "||example.com^$xmlprune=/a,b/",
        "||example.com^$jsonprune=\\$..[?(has @.a,@.b)]",
        "||example.com^$hls=/#UPLYNK-SEGMENT:.*\\,ad/",
        // Options the known set had omitted.
        "||example.com^$inline-font",
        "||example.com^$beacon",
        "||example.com^$mp4",
        "||example.com^$queryprune=x",
        // AdGuard JavaScript injection is not a CSS selector.
        "example.com#%#window.__adblock = true; // don't show",
        "example.com#@%#var x = '(';",
        "example.com#$#//scriptlet('abort-on-property-read', 'x')",
    ] {
        assert!(f(valid).is_none(), "{:?} flagged as {:?}", valid, f(valid).map(|p| p.reason));
    }
    // Genuine defects still register.
    assert_eq!(f("||example.com^$fakeopt").unwrap().reason, "unknown option");
    assert_eq!(f("||example.com$").unwrap().reason, "option marker with no options");
}

#[test]
fn test_suggest_option_is_deterministic() {
    // Both option sets iterate in a randomised order; a tie must not make the
    // suggestion vary between runs, or CI output stops being reproducible.
    for _ in 0..64 {
        assert_eq!(crate::suggest_option("thrid-party"), Some("third-party"));
        assert_eq!(crate::suggest_option("beacom"), crate::suggest_option("beacom"));
    }
}

#[test]
fn test_has_text_merges_across_separators_and_dedups() {
    use crate::fop_sort::combine_has_text_rules as c;
    // The case this was written for: a part-merged group, where one rule is
    // already the regex and the others are the plain texts it covers.
    for sep in ["##", "#?#"] {
        let base = format!(r#"bol.com{}[data-bltgi*="ProductList_"]"#, sep);
        let got = c(vec![
            format!("{}:has-text(/Gesponsord|Sponsorisé/)", base),
            format!("{}:has-text(Sponsorisé)", base),
            format!("{}:has-text(Gesponsord)", base),
        ]);
        assert_eq!(got, vec![format!("{}:has-text(/Gesponsord|Sponsorisé/)", base)], "{}", sep);
    }
    // Merging is idempotent now: running it again must not grow the regex.
    let once = c(vec![
        "a.com##.x:has-text(A)".into(),
        "a.com##.x:has-text(B)".into(),
    ]);
    assert_eq!(once, vec!["a.com##.x:has-text(/A|B/)".to_string()]);
    assert_eq!(c(once.clone()), once);

    // Exceptions are never merged at all. An exception cancels a hiding rule
    // by matching its selector text, so folding two of them leaves neither
    // original string in existence and the rules they cancelled are no longer
    // excepted. A hiding rule stands alone, so merging those is safe.
    // An ID selector is the case that matters: `#@#` + `#ad` puts two `#`
    // together, and a scan that steps over the separator it cannot merge finds
    // that pair and splits there instead -- merging the exception after all.
    // `#$#`/`#%#` inject CSS and JavaScript and must not be touched either.
    for sep in ["#@#", "#@?#", "#$#", "#%#", "#@$#", "#@%#"] {
        for selector in ["#ad", ".x", "[data-x=\"y\"]"] {
            let untouched = vec![
                format!("a.com{}{}:has-text(A)", sep, selector),
                format!("a.com{}{}:has-text(B)", sep, selector),
            ];
            assert_eq!(c(untouched.clone()), untouched, "{} {}", sep, selector);
        }
    }
    // A hiding rule with an ID selector still merges.
    assert_eq!(
        c(vec!["a.com###ad:has-text(A)".into(), "a.com###ad:has-text(B)".into()]),
        vec!["a.com###ad:has-text(/A|B/)".to_string()]
    );
    // ...nor is a hiding rule ever folded together with an exception.
    let mixed = c(vec![
        "a.com##.x:has-text(A)".into(),
        "a.com#@#.x:has-text(B)".into(),
    ]);
    assert_eq!(mixed.len(), 2);

    // A `|` inside a group belongs to its alternative, not to the split.
    let grouped = c(vec![
        "a.com##.x:has-text(/(a|b)c/)".into(),
        "a.com##.x:has-text(d)".into(),
    ]);
    assert_eq!(grouped, vec!["a.com##.x:has-text(/(a|b)c|d/)".to_string()]);

    // A nested `:has(span:has-text(x))` splits badly -- the pattern is lazy, so
    // the base keeps an unclosed `(` and the argument an extra `)`. Merging on
    // that split produced a rule one bracket short whose regex hunted for a
    // literal `)`; such a group is left alone instead.
    let nested = vec![
        r#"instagram.com#?#div[style="x"]:has(span:has-text(Paid partnership with ))"#.to_string(),
        r#"instagram.com#?#div[style="x"]:has(span:has-text(Paid partnership))"#.to_string(),
    ];
    assert_eq!(c(nested.clone()), nested);

    // CSS and JS injection carry no selector to merge.
    let inject = c(vec![
        "a.com#$#.x:has-text(A)".into(),
        "a.com#$#.x:has-text(B)".into(),
    ]);
    assert_eq!(inject.len(), 2);
}

#[test]
fn test_missing_anchor_is_flagged() {
    use crate::fop_rules::check_rule as f;
    // The rule that prompted this: `||` forgotten, so it matches the name
    // anywhere -- `rbush.shop^` also blocks `lampedburbush.shop`.
    // A dotless token is the same mistake without even a domain in it. The
    // form does not occur once in 609k lines of real lists.
    // With or without the `^`, and with options hanging off it.
    for garbage in [
        "fdfdgfgdgfd^", "ffgdfgdfgd^", "wxyzzzq^", "kjhgfdsz^",
        "fdgfgdfgd", "kjhgfdsz", "fdgfgdfgd$third-party",
    ] {
        let p = f(garbage).expect(garbage);
        // Its own wording: there is no host here, so "did you mean ||host^"
        // would be guessing at an intent the rule does not show.
        assert_eq!(p.reason, "unanchored pattern with no domain -- matches this text anywhere");
        assert!(!p.removable);
    }
    // A token that reads as a word is left alone, however unusual: every one
    // of these is a pattern someone could reasonably write, and under
    // --remove-bad-rules a false positive is a deleted rule.
    for ok in [
        "doubleclick^", "prebid^", "sponsored^", "adsbygoogle^", "300x250^",
        "click^", "adserv^", "_ads^", "-ads^", "a^", "cdn^",
        // The same words without a `^` are patterns too.
        "doubleclick", "sponsored", "prebid", "adserver", "banner",
        // Digits or punctuation put a token outside what this can judge:
        // `300x250` is real, and `df334sdf` is not, but nothing here can
        // tell them apart, so neither is flagged.
        "df334sdf", "fdsgfgd@!", "300x250",
        // ...as is anything the author anchored or gave a path.
        "||adserv^", "/ads^", "adserv^somepath", ".adserv^", "|adserv^",
    ] {
        assert!(f(ok).is_none(), "{:?} flagged as {:?}", ok, f(ok).map(|p| p.reason));
    }
    for missing in [
        "rbush.shop^",
        "rbush.shop^$third-party",
        "arketing.indianadunes.com^",
        "sub.example.co.nz^",
    ] {
        let p = f(missing).expect(missing);
        assert!(p.reason.contains("no || anchor"), "{}: {}", missing, p.reason);
        // Reported as advice, which keeps it out of the CI exit code while
        // still being removed by --remove-bad-rules.
        assert!(!p.removable, "{} should be reported as advice", missing);
    }
    // An author who chose their matching is left alone.
    for ok in [
        "||rbush.shop^",
        "||rbush.shop^$third-party",
        "@@||rbush.shop^",
        "|http://rbush.shop^",
        ".rbush.shop^",
        "*.rbush.shop^",
        "-ad.com^",
        "/ads/banner^",
        "ads.js^",
        "example.com##.ad",
    ] {
        assert!(f(ok).is_none(), "{:?} flagged as {:?}", ok, f(ok).map(|p| p.reason));
    }
}

#[test]
fn test_has_text_merge_refuses_what_it_cannot_fold() {
    use crate::fop_sort::combine_has_text_rules as c;
    // A lone `/` is not a regex; treating it as one sliced [1..0] and aborted
    // the whole sort on any list containing a truncated rule.
    let slash = c(vec!["a.com##.x:has-text(/)".into(), "a.com##.x:has-text(B)".into()]);
    assert_eq!(slash, vec!["a.com##.x:has-text(//|B/)".to_string()]);

    // A regex carrying flags cannot be folded -- its flags would not survive,
    // and `/foo/i` joined as text becomes a literal search for six characters.
    let flagged = vec![
        "a.com##.x:has-text(/foo/i)".to_string(),
        "a.com##.x:has-text(bar)".to_string(),
    ];
    assert_eq!(c(flagged.clone()), flagged);

    // An empty alternative matches everything; dropping it narrows the rule.
    let empty_alt = vec![
        "a.com##.x:has-text(/foo|/)".to_string(),
        "a.com##.x:has-text(bar)".to_string(),
    ];
    assert_eq!(c(empty_alt.clone()), empty_alt);

    // A group that is put back must keep its place. Only the first member
    // advanced the position counter, so offsetting the rest by their index
    // collided with later lines and the stable sort interleaved them.
    assert_eq!(
        c(vec![
            "a.com##.x:has-text(/foo/i)".into(),
            "a.com##.x:has-text(bar)".into(),
            "b.com##.y".into(),
        ]),
        vec![
            "a.com##.x:has-text(/foo/i)".to_string(),
            "a.com##.x:has-text(bar)".to_string(),
            "b.com##.y".to_string(),
        ]
    );

    // Ordinary English text is literal, so an apostrophe is a character rather
    // than an open quote and must not block the merge.
    assert_eq!(
        c(vec!["a.com##.x:has-text(Don't miss)".into(), "a.com##.x:has-text(Bar)".into()]),
        vec!["a.com##.x:has-text(/Don't miss|Bar/)".to_string()]
    );

    // The separator is found at the first `#`, not wherever one is listed, so
    // a selector containing `#?#` in an attribute is not split inside it.
    let attr = vec![
        r##"a.com##a[href*="#?#top"]:has-text(A)"##.to_string(),
        r##"a.com##a[href*="#?#top"]:has-text(B)"##.to_string(),
    ];
    assert_eq!(c(attr), vec![r##"a.com##a[href*="#?#top"]:has-text(/A|B/)"##.to_string()]);
}

#[test]
fn test_missing_anchor_only_for_host_rules() {
    use crate::fop_rules::check_rule as f;
    // A path after the `^` means it is not a host rule, and the advice would
    // describe it wrongly.
    assert!(f("example.com^somepath").is_none());
    assert!(f("example.com^*/ads").is_none());
    assert!(f("example.com^").is_some());
    assert!(f("example.com^|").is_some());
    // An option value holding a space fails OPTION_PATTERN, which used to skip
    // the pattern half entirely.
    assert!(f("example.com^$csp=script-src 'none'").is_some());
    assert!(f("||example.com^$csp=script-src 'none'").is_none());
}

#[test]
fn test_split_options_matches_the_pattern_it_replaced() {
    use crate::fop_rules::check_rule as f;
    // A `$` that is not an option marker: the option keys would have to hold
    // characters no key may carry.
    for not_a_rule in [
        "export PATH=$PATH:/usr/bin",
        "  run: echo \"$GITHUB_SHA\"",
        "some: $value",
        "let x = format!(\"{}\", $y);",
    ] {
        assert!(f(not_a_rule).is_none(), "{:?} flagged", not_a_rule);
    }
    // Values that legitimately carry `$`, commas or spaces.
    for valid in [
        "||example.com^$removeparam=/^utm$/",
        "||example.com/script.js$replace=/(foo)bar/$1baz/",
        "||example.com^$xmlprune=/a,b/",
        "||example.com^$csp=script-src 'none'",
        "||example.com^$third-party",
        "||example.com^$inline-font",
    ] {
        assert!(f(valid).is_none(), "{:?} flagged as {:?}", valid, f(valid).map(|p| p.reason));
    }
    // Defects still register, by each of the routes through the splitter.
    assert_eq!(f("||example.com^$fakeopt").unwrap().reason, "unknown option");
    assert_eq!(f("||example.com$").unwrap().reason, "option marker with no options");
    assert_eq!(f("||example.com^$domain=").unwrap().reason, "option with no value");
    assert_eq!(f("||x.com^$third-party,").unwrap().reason, "empty option");
    assert_eq!(f("||example.com^$domain=a.com|").unwrap().reason, "empty entry in option value");
}

#[test]
fn test_split_options_agrees_with_the_regex() {
    use crate::fop_rules::split_options;
    // `OPTION_PATTERN` is still the sorter's definition of an option list, so
    // the scan that replaced it here is held to producing the same split. It
    // agrees on all 609k lines of EasyList and the region lists; these are the
    // shapes that are rare or absent there.
    for line in [
        // Not option markers at all.
        "export PATH=$PATH:/usr/bin", "  run: echo \"$GITHUB_SHA\"", "some: $value",
        "let x = format!(\"{}\", $y);", "echo $?", "$", "$$", "a$", "$,",
        // A value carrying `$`, so the marker is not the last one -- the regex
        // found this by backtracking, and the scan walks the same candidates.
        "||x^$removeparam=/^utm$/", "||x^$replace=/(a)b/$1c/", "a$b$c",
        // Empty values belong to the anchored fallback, not to this split.
        "$a=", "||x^$domain=",
        // Ordinary lists.
        "||x^$third-party", "||x^$domain=a.com|~b.com,important", "||x^$~third-party",
        "||x^$xmlprune=/a,b/", "||x^$UPPER", "||x^$a_b", "||x^$a-b", "||x^$1",
        // Whitespace in a value, which neither accepts.
        "||x^$csp=script-src 'none'", "||x^$a b", "||x^$a=b c", "||x^$ ",
        // Escaped markers.
        "x\\$y$third-party",
    ] {
        let scanned = split_options(line);
        let matched = crate::OPTION_PATTERN
            .captures(line)
            .map(|c| (c.get(1).unwrap().as_str(), c.get(2).unwrap().as_str()));
        assert_eq!(scanned, matched, "{:?}", line);
    }
}
