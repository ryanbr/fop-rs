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
use crate::fop_datestamp::{is_timestamp_line, is_version_line};

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
fn test_typo_fix_touches_network_rules_only() {
    // A line carrying `##` or `$$` is read as cosmetic by ABP, uBO and AdGuard
    // alike, so the `$option.option` fix -- which rewrites network options --
    // leaves it alone. These were once "repaired" on the theory that a network
    // rule could hold `##` in its path; none can (a URL fragment is never part
    // of a request) and none appears in 2.2M lines of real lists, while the
    // narrower test that theory required corrupted a real uAssets scriptlet.
    for line in ["||a.com/$$p^$third-party.script", "||a.com/a##b^$third-party.script"] {
        assert_eq!(filter_tidy(line, false), line, "rewritten: {}", line);
    }
    // A genuine cosmetic rule is still exempt.
    assert_eq!(
        filter_tidy("example.com##div[data-x=\"a.b$c.d\"]", false),
        "example.com##div[data-x=\"a.b$c.d\"]"
    );

    // Regex-domain cosmetic rules too: a regex host anchor ends in `$`, which
    // reads as an option marker, but the separator makes the line cosmetic.
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

    // Any address, not only the two a blocklist null-routes with: a hosts
    // file's own preamble is written in neither. These are StevenBlack's,
    // which this used to assert were not entries -- and so mangled.
    assert!(is_localhost_entry("255.255.255.255 broadcasthost"));
    assert!(is_localhost_entry("192.168.1.1 domain.com"));
    assert!(is_localhost_entry("::1 localhost"));
    assert!(is_localhost_entry("::1 ip6-localhost"));
    assert!(is_localhost_entry("fe00::0 ip6-localnet"));
    assert!(is_localhost_entry("ff02::3 ip6-allhosts"));
    assert!(is_localhost_entry(":: ab"));

    // Invalid entries
    assert!(!is_localhost_entry("# comment"));
    assert!(!is_localhost_entry("domain.com"));
    assert!(!is_localhost_entry("0.0.0.0"));
    assert!(!is_localhost_entry("0.0.0.0 "));
    assert!(!is_localhost_entry("127.0.0.1"));
    assert!(!is_localhost_entry(""));
    // Address-shaped but not an address.
    assert!(!is_localhost_entry("999.1.1.1 domain.com"));
    assert!(!is_localhost_entry("1.2.3 domain.com"));
    assert!(!is_localhost_entry("::zz host"));
    // A filter rule that happens to carry a space must not read as one.
    assert!(!is_localhost_entry("example.com##div > p"));
    assert!(!is_localhost_entry("*$csp=default-src 'none'"));
    assert!(!is_localhost_entry("! a comment with spaces"));
    assert!(!is_localhost_entry("example.com#$#body { color: red; }"));
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
    // And a trailing `*` then closes the last option's value, not the pattern.
    // `domain=isohunt.*` was written back as `domain=isohunt.`, a host with a
    // trailing dot that matches nothing; the second rule lost the only domain
    // it had. Both are real rules from uAssets' filters-2021.txt.
    for rule in [
        "*$csp=script-src *,domain=isohuntz.*|isohunt.*|myisohunt.*",
        "*$csp=script-src *,domain=torrentproject2.*",
        "||t.com^$csp=a b,domain=x.*",
        "*$csp=a *,domain=x.*|y.com",
    ] {
        assert_eq!(remove_unnecessary_wildcards(rule), rule, "wildcard trimmed off an option");
    }
    // A pattern reaching here on its own still loses its trailing `*`, which is
    // the whole point of the function: the guard keys on the option separator,
    // not merely on the rule holding a `*`.
    assert_eq!(remove_unnecessary_wildcards("||example.com^*"), "||example.com^");
    assert_eq!(remove_unnecessary_wildcards("||example.com/a\\$b*"), "||example.com/a\\$b");
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
    // ...but with options and no separator it is a substring pattern, which is
    // how whole files are written -- easyprivacy_general_emailtrackers.txt
    // holds 319 of them and not one anchored rule.
    assert!(f("domain.com$third-party").is_none());
    assert!(f("img.promio-connect.com$image").is_none());
    // ...and with a separator *and* options it is left alone too: writing an
    // option is a deliberate act, and --remove-bad-rules deletes advice.
    assert!(f("domain.com^$third-party").is_none());
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
    let base = r#"bol.com##[data-bltgi*="ProductList_"]"#;
    let got = c(vec![
        format!("{}:has-text(/Gesponsord|Sponsorisé/)", base),
        format!("{}:has-text(Sponsorisé)", base),
        format!("{}:has-text(Gesponsord)", base),
    ]);
    assert_eq!(got, vec![format!("{}:has-text(/Gesponsord|Sponsorisé/)", base)]);
    // Merging is idempotent now: running it again must not grow the regex.
    let once = c(vec![
        "a.com##.x:has-text(A)".into(),
        "a.com##.x:has-text(B)".into(),
    ]);
    assert_eq!(once, vec!["a.com##.x:has-text(/A|B/)".to_string()]);
    assert_eq!(c(once.clone()), once);

    // Only `##` merges. An exception cancels a hiding rule by matching its
    // selector text, so folding two of them leaves neither original string in
    // existence and the rules they cancelled are no longer excepted. `#?#` is
    // out for a different reason: merging rewrites `:-abp-contains(text)` into
    // `:-abp-contains(/regex/)`, which assumes whatever reads that separator
    // takes a regex there. A hiding rule stands alone, so merging those is
    // safe.
    // An ID selector is the case that matters: `#@#` + `#ad` puts two `#`
    // together, and a scan that steps over the separator it cannot merge finds
    // that pair and splits there instead -- merging the exception after all.
    // `#$#`/`#%#` inject CSS and JavaScript and must not be touched either.
    // `#$?#` and `#@$?#` included on purpose: they are the only 4- and 5-byte
    // separators, reached through the deepest branches of the matcher. If one
    // of those ever failed to match, the scan would step past it onto the `##`
    // its trailing `#` forms with the `#ad` selector -- and merge two AdGuard
    // exceptions.
    for sep in ["#@#", "#@?#", "#$#", "#%#", "#@$#", "#@%#", "#$?#", "#@$?#", "#?#"] {
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
        // Options do not excuse mash, though they do excuse a real hostname.
        // A leading boundary character is not a disguise: judge what follows.
        "+dsfsdffdsfds", "-dffgdfdfs", "_dffgdfdfs", "-fdfdgfgdgfd^",
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
        // Real lines from easylist_general_block.txt, which is 973 unanchored
        // substring patterns against 3 anchored rules -- the shape most at
        // risk from these checks.
        "&rb=&uuid=$third-party", "&subaffid=%$subdocument,third-party",
        "-ad-manager/$~stylesheet", "-ad-sidebar.$image",
        "-ad.jpg.pagespeed.$image", "-ads-manager/$domain=~wordpress.org",
        "-ads/assets/$script,domain=~web-ads.org", "-assets/ads.$~script",
        // Those same characters in front of a real pattern. `-ad.com^` keeps
        // its leading boundary on purpose, so it is not advised to anchor.
        "-ad-banner-", "-ads", "_ads", "+ads", "-adserver", "--", "-", "+",
        "-ad.com^", "_ad.com^", "+ad.com^",
        // ...as is anything the author anchored or gave a path.
        "||adserv^", "/ads^", "adserv^somepath", ".adserv^", "|adserv^",
    ] {
        assert!(f(ok).is_none(), "{:?} flagged as {:?}", ok, f(ok).map(|p| p.reason));
    }
    for missing in [
        "rbush.shop^",
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

    // A flagged argument does not pull an unflagged one under its flags.
    // `Sponsored` is a case-sensitive match and `/Protect your privacy/i` is
    // not; folding them gave `/…|Sponsored/i`, which hid strictly more than
    // the two rules did. These are uAssets' own rules on torrentz2, written
    // separately and saying nothing about each other. Either order refuses.
    let widening = vec![
        "a.com##.x:has-text(/Protect your privacy/i)".to_string(),
        "a.com##.x:has-text(Sponsored)".to_string(),
    ];
    assert_eq!(c(widening.clone()), widening);
    let reversed: Vec<String> = widening.iter().rev().cloned().collect();
    assert_eq!(c(reversed.clone()), reversed);
    // Arguments already agreeing on their flags fold as before, as do two that
    // are both case-sensitive -- plain text beside an unflagged regex.
    assert_eq!(
        c(vec![
            "a.com##.x:has-text(/foo/i)".into(),
            "a.com##.x:has-text(/bar/i)".into(),
        ]),
        vec!["a.com##.x:has-text(/foo|bar/i)".to_string()]
    );
    assert_eq!(
        c(vec!["a.com##.x:has-text(/foo/)".into(), "a.com##.x:has-text(bar)".into()]),
        vec!["a.com##.x:has-text(/foo|bar/)".to_string()]
    );
    // Two different flag sets have no single form to merge into.
    let mixed_flags = vec![
        "a.com##.x:has-text(/foo/i)".to_string(),
        "a.com##.x:has-text(/bar/m)".to_string(),
    ];
    assert_eq!(c(mixed_flags.clone()), mixed_flags);
    // Agreeing flags merge and keep them: `/a|b/i` means what both meant.
    assert_eq!(
        c(vec![
            "a.com##.x:has-text(/Protect your privacy/i)".into(),
            "a.com##.x:has-text(/Sponsored/i)".into(),
        ]),
        vec!["a.com##.x:has-text(/Protect your privacy|Sponsored/i)".to_string()]
    );
    // ...and merging stays idempotent with flags attached.
    let once = c(vec![
        "a.com##.x:has-text(/A/i)".into(),
        "a.com##.x:has-text(/B/i)".into(),
    ]);
    assert_eq!(once, vec!["a.com##.x:has-text(/A|B/i)".to_string()]);
    assert_eq!(c(once.clone()), once);
    // An empty alternative still stops the whole group, flags or not.
    let empty_flagged = vec![
        "a.com##.x:has-text(/foo|/i)".to_string(),
        "a.com##.x:has-text(/bar/i)".to_string(),
    ];
    assert_eq!(c(empty_flagged.clone()), empty_flagged);

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
            "a.com##.x:has-text(/foo|/)".into(),
            "a.com##.x:has-text(bar)".into(),
            "b.com##.y".into(),
        ]),
        vec![
            "a.com##.x:has-text(/foo|/)".to_string(),
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
    // Options mean the author chose the matching, so the host is left alone
    // whether or not the option value parses. ABP's anti-circumvention list
    // publishes 13 of these and --remove-bad-rules was deleting them.
    assert!(f("example.com^$csp=script-src 'none'").is_none());
    assert!(f("||example.com^$csp=script-src 'none'").is_none());
    assert!(f("billboard.com^$csp=script-src-attr 'none'").is_none());
    assert!(f("host-cdn.net^$image,redirect-rule=32x32.png,domain=maxstream.video").is_none());
    // Mash is not a choice, so options do not excuse it.
    assert!(f("fdfdgfgdgfd^$third-party").is_some());
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
    use crate::fop_rules::{split_options, split_options_as_pattern};
    // `OPTION_PATTERN` is the definition of an option list that the sorter
    // shipped with, so the scan that replaced it is held to producing the same
    // split. `split_options_as_pattern` is the one that has to match it
    // exactly, and does on all 2,619,918 lines of four corpora; these are the
    // shapes that are rare or absent there.
    //
    // `split_options` is deliberately not the same. It tokenises on unescaped
    // commas, so it accepts a value holding `\,` where the regex, reading a
    // value as `[^,\s]+`, stops at the escape and declines the line. 282 rules
    // across those corpora are shaped that way. Both contracts are pinned
    // below: the sorter keeps the regex's answer, and the addition checks keep
    // the one that understands the escape.
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
        let matched = crate::OPTION_PATTERN
            .captures(line)
            .map(|c| (c.get(1).unwrap().as_str(), c.get(2).unwrap().as_str()));
        assert_eq!(split_options_as_pattern(line), matched, "as_pattern: {:?}", line);
        assert_eq!(split_options(line), matched, "split_options: {:?}", line);
    }
    // Where the two part company, and why the sorter uses the first.
    for line in [
        "||x^$replace=/a\\,b/c/",
        "||abcya.com/client/main-*.js$script,replace=/\\,n.src=s.ri.adDetect//",
    ] {
        assert!(crate::OPTION_PATTERN.captures(line).is_none(), "{line}");
        assert_eq!(split_options_as_pattern(line), None, "as_pattern took it: {line}");
        assert!(split_options(line).is_some(), "split_options declined it: {line}");
    }
}

#[test]
fn test_parse_added_lines_handles_rules_starting_with_plus() {
    use crate::fop_git::parse_added_lines;
    // A rule beginning with `+` reaches the diff as `+++...`, which looks like
    // the `+++ b/file` header unless the space is required. Getting that wrong
    // dropped the rule *and* stopped the line counter, so every later finding
    // named a line one too low and failed the check that guards removal.
    let diff = "diff --git a/easylist/l.txt b/easylist/l.txt\n\
                --- a/easylist/l.txt\n\
                +++ b/easylist/l.txt\n\
                @@ -1,0 +2,6 @@\n\
                +++sdfsdffsdfds\n\
                ++sdsdffsdfds\n\
                +--sdfdsfdfsdfs\n\
                +-sfdgdfsfds\n\
                +\n\
                +++ b/evil.txt\n";
    let added = parse_added_lines(diff);
    let got: Vec<(usize, &str)> = added.iter().map(|a| (a.line_num, a.content.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (2, "++sdfsdffsdfds"),
            (3, "+sdsdffsdfds"),
            (4, "--sdfdsfdfsdfs"),
            (5, "-sfdgdfsfds"),
            // line 6 is the added blank line, dropped as empty content but it
            // must still advance the counter, or everything after it shifts
            (7, "++ b/evil.txt"),
        ]
    );
    // A rule reading `++ b/evil.txt` arrives as `+++ b/evil.txt`. Inside a
    // hunk that is content, not a header -- taking it as one repointed every
    // later addition at a file the commit never touched.
    assert!(added.iter().all(|a| a.file == "easylist/l.txt"));
}

#[test]
fn test_parse_added_lines_refuses_combined_diffs() {
    use crate::fop_git::parse_added_lines;
    // A merge in progress makes git emit two status columns and `@@@` hunk
    // headers. Nothing here parses those, so the line numbers and content
    // would both be wrong; reporting nothing is the honest answer.
    let diff = "diff --cc l.txt\n\
                index 111,222..333\n\
                @@@ -1,2 -1,2 +1,4 @@@\n\
                ++both.com^\n\
                 +one.com^\n";
    assert!(parse_added_lines(diff).is_empty());
}

#[test]
fn test_space_in_pattern_only_for_standard_rules() {
    use crate::fop_rules::check_rule as f;
    // A network rule's pattern never holds a space: across 609k lines of
    // EasyList and the region lists, not one does. In a standard rule that is
    // a defect -- it can never match.
    for broken in [
        "||exa mple.com^",
        "||exa mple.com^$third-party",
        "exa mple.com^",
        "@@||exa mple.com^",
        "|http://exa mple.com",
    ] {
        let p = f(broken).expect(broken);
        assert_eq!(p.reason, "space in the pattern -- uBO and AdGuard will not match this");
        // Advice rather than a defect, so it does not fail a CI build:
        // `filter_tidy` strips these spaces on the sorting pass anyway. It is
        // still deleted by --remove-bad-rules, which takes everything flagged.
        assert!(!p.removable, "{} should be advice", broken);
    }
    // An anchor has to be followed by rule text. A patch hunk header and a
    // markdown table row both open with one and are not rules.
    for not_a_rule in [
        "@@ -3,6 +3,9 @@ import Foundation",
        "| Option | Description |",
        "|| echo fallback",
        // A regex filter keeps its spaces on purpose.
        r#"@@/^https?:\/\/[^ ]+\/ads\//$script"#,
        r#"|/re gex/$script"#,
        // A `^` mid-string is not a separator, and nothing else here is a rule.
        r#"grep "^foo bar" file"#,
    ] {
        assert!(f(not_a_rule).is_none(), "{:?} flagged as {:?}", not_a_rule, f(not_a_rule).map(|p| p.reason));
    }
    // Everywhere else a space is ordinary and must be left alone. `$csp` is
    // the case that matters: every one of the 41 rules in those lists with a
    // space in an option value is a CSP directive.
    for ok in [
        "||example.com^$csp=script-src 'none'",
        "$csp=child-src 'none'; frame-src 'self' *",
        "||example.com^$replace=/foo bar/baz/",
        // Real `$csp=` rules, whose values are full of spaces. These parse as
        // options, so the pattern half is known and holds none.
        "$csp=child-src 'none'; frame-src 'self' *; worker-src 'none',domain=fileone.tv",
        "||thegay.com^$csp=default-src 'self' *.ahcdn.com fonts.gstatic.com https://thegay.com",
        // Other option values that may carry spaces or awkward punctuation.
        "||example.com^$permissions=autoplay=()|geolocation=()",
        "||example.com^$removeheader=set-cookie",
        r#"||example.com^$hls=/#UPLYNK-SEGMENT:.*\,ad/"#,
        r#"||example.com^$jsonprune=\$..[?(has @.a)]"#,
        "*$doc,replace=/popunder//,to=fullxh.com|megaxh.com",
        // Real uBO rules whose `$replace=` rewrite carries HTML, so the option
        // list does not parse and the pattern half is unknowable. Flagging
        // these called three valid uAssets rules defects.
        r#"||dragontea.ink^$document,replace=/(var tea='\{"ct":"[0-9a-f]+"\}';)/$1document.write('<link rel="stylesheet" href="x">')/"#,
        r#"||wiki.yjsnpi.nu/comments/$script,replace=/(;\}\}function [A-Za-z]+\([A-Za-z]?\))/$1 var x = 1;/"#,
        // A hosts entry, which fop is given in localhost mode.
        "127.0.0.1 example.com",
        // Cosmetic selectors are full of spaces, and carry `^=` attribute
        // operators that must not read as separators.
        "example.com##div > span",
        "example.com##.a:has-text(Buy now)",
        "example.com#%#window.x = 1;",
        r#"mylocation.org##.info a[href^="https://go.expressvpn.com/c/"]"#,
        r#"example.com##div > span a[href^="/ads"]"#,
        r#"example.com#@#.info a[href^="https://x.com/"]"#,
        r#"example.com#?#.a:has-text(Buy now) > .b"#,
        r#"example.com#$#.ad { display: none !important; }"#,
        r#"example.com##[data-x="a b c"]"#,
        // Not rule-shaped, so not ours to judge.
        "++ dfsdsfdsf",
        "! a comment with spaces",
        "[Adblock Plus 2.0]",
        // A `^` mid-string is a regex anchor in someone's shell, not a
        // separator -- only a trailing one says "rule".
        "sed -i \"s/^version = .*/version = \\\"$V\\\"/\"",
        "  run: echo \"$GITHUB_SHA\"",
    ] {
        assert!(f(ok).is_none(), "{:?} flagged as {:?}", ok, f(ok).map(|p| p.reason));
    }
}

/// AdGuard and uBO syntax must survive the addition checks untouched.
#[test]
fn test_engine_specific_rules_are_left_alone() {
    use crate::fop_rules::check_rule as f;
    let adguard = [
        "example.com#$#.ad { display: none!important; }",
        "example.com#$?#.ad:has(.x) { remove: true; }",
        "example.com#@$#.ad { display: none; }",
        "example.com#@$?#.ad { remove: true; }",
        "example.com#%#window.__adg = 1;",
        "example.com#@%#window.__adg = 1;",
        "example.com#%#//scriptlet('abort-on-property-read', 'ads')",
        r#"example.com$$script[tag-content="ad config"]"#,
        r#"example.com$@$script[tag-content="ad"]"#,
        "[$path=/page/]example.com##.ad",
        "[$domain=example.com]##.ad",
        "||example.com^$removeheader=refresh",
        "||example.com^$stealth=referrer",
        "||example.com^$app=org.example.app",
        "||example.com^$network",
        "||example.com^$important,third-party",
        "||example.com^$badfilter",
        "||example.com^$denyallow=a.com|b.com",
        "||example.com^$jsonprune=\\$..[?(has @.ads)]",
        "||example.com^$hls=/#UPLYNK-SEGMENT:.*\\,ad/",
        "@@||example.com^$genericblock,generichide",
    ];
    let ubo = [
        "example.com##+js(aopr, ads)",
        "example.com##^script:has-text(adsbygoogle)",
        "example.com#@#+js(aopr, ads)",
        "example.com#?#.ad:has-text(Sponsored)",
        "example.com##.ad:matches-css(display: block)",
        "example.com##.ad:xpath(//div[@id=\"x\"])",
        "example.com##.ad:upward(2)",
        "example.com##.ad:watch-attr(class)",
        "example.com##.ad:min-text-length(100)",
        "example.com##.ad:style(display: none !important)",
        "||example.com^$removeparam=utm_source",
        "||example.com^$redirect=noopjs",
        "||example.com^$redirect-rule=noopmp3-0.1s",
        "||example.com^$csp=script-src 'none'",
        "||example.com^$1p,strict3p",
        "||example.com^$doc,ghide",
        "||example.com^$ehide,shide",
        "||example.com^$cname",
        "||example.com^$inline-script,inline-font",
        "||example.com^$empty,mp4,popunder",
        "||example.com^$method=get|post",
        "||example.com^$to=a.com,from=b.com",
        "||example.com^$ipaddress=1.2.3.4",
        "||example.com^$header=via:1.1",
        "||example.com^$urlskip=/^/ -3",
        "||example.com^$uritransform=/x/y/",
        "/^https?:\\/\\/ads\\d+\\.example\\.com\\//$script",
        "!#include filters/other.txt",
        "!#if env_chromium",
    ];
    // Neither engine's own syntax is any of fop's business here: the checks
    // look for rules that cannot work, and every one of these does.
    for (engine, set) in [("AdGuard", &adguard[..]), ("uBO", &ubo[..])] {
        for l in set {
            assert!(f(l).is_none(), "{} rule flagged as {:?}: {}", engine, f(l).map(|p| p.reason), l);
        }
    }
}

#[test]
fn test_requestheader_is_known() {
    use crate::fop_rules::check_rule as f;
    // Live in uAssets filters-2026.txt. Unknown options are a defect, so this
    // was a rule --remove-bad-rules would have deleted and CI would have
    // failed on -- the same shape as the inline-font/beacon gap before it.
    let rule = "||workers.dev/index.js$script,3p,requestheader=Cookie:*doubleclick.net*";
    assert!(f(rule).is_none(), "{:?}", f(rule).map(|p| p.reason));
    assert!(crate::is_known_option("requestheader=Cookie:*x*"));
}

#[test]
fn test_header_option_values_keep_their_spaces() {
    use crate::fop_sort::filter_tidy;
    // `filter_tidy` strips whitespace from network rules, which is right for
    // `|| x .com ^` and wrong for a header value: `content-type:text/html;
    // charset=utf-8` means something different without its space.
    for rule in [
        "||x.com^$header=content-type:text/html; charset=utf-8",
        "||x.com^$responseheader=set-cookie: a",
        "||x.com^$requestheader=Cookie: a",
        "||x.com^$permissions=autoplay=() geolocation=()",
        "||x.com^$csp=script-src 'none'",
    ] {
        assert!(filter_tidy(rule, false).contains(' '), "space stripped from {}", rule);
    }
    // An ordinary rule still has its errant spaces taken out.
    assert_eq!(filter_tidy("|| x .com ^$script", false), "||x.com^$script");
    // The rule from uAssets that prompted this keeps both wildcards and its
    // colon through a tidy.
    let live = "||workers.dev/index.js$script,3p,requestheader=Cookie:*doubleclick.net*";
    assert!(filter_tidy(live, false).contains("requestheader=Cookie:*doubleclick.net*"));
}

#[test]
fn test_localhost_entries_keep_their_space() {
    use crate::fop_sort::is_localhost_entry;
    // The space between IP and host is the syntax of a hosts entry, and
    // `filter_tidy` strips whitespace from anything that is not an element
    // rule -- which turned `0.0.0.0 keep.com` into `0.0.0.0keep.com` and broke
    // every hosts file fop sorted in localhost mode, in v5.5.0 too. Such a
    // line is now passed through as written.
    for entry in [
        "0.0.0.0 keep.com",
        "127.0.0.1 tracker.net",
        "0.0.0.0\ttabbed.com",
        "0.0.0.0 sub.example.co.nz",
    ] {
        assert!(is_localhost_entry(entry), "{}", entry);
        // ...and the checks have nothing to say about one either, mangled or
        // not: an IP and a host separated by whitespace is not a rule shape.
        assert!(crate::fop_rules::check_rule(entry).is_none(), "{}", entry);
    }
    // The mangled form is what the checks used to see, and it reads as a bare
    // domain -- which is how a whole hosts file came to be deleted by
    // --remove-bad-rules.
    assert!(crate::fop_rules::check_rule("0.0.0.0keep.com").is_some());
}

#[test]
fn test_abp_snippet_body_is_not_a_selector() {
    use crate::fop_rules::check_rule as f;
    // `#$#` snippet arguments carry regex literals and quoted strings whose
    // brackets are data. All three are published in ABP's anti-circumvention
    // list and were being deleted as "unbalanced brackets in selector".
    for snippet in [
        r"kaliscan.*#$#abort-current-inline-script document.createElement /l\\.parentNode\\.insertBefore\\(s/;",
        r#"tvnz.co.nz#$#replace-fetch-response /"adType":"ssai"/ "adType":"none"; replace-fetch-response /"aopUrl":"[^"]*"/ '"aopUrl":""'"#,
        r#"pluto.tv#$#replace-xhr-response /<Period[^>]*?id="[0-9a-fA-F]+-[0-9]+"[^>]*>.+?<[/]Period>/ '' 'urn:mpeg:dash'"#,
        "testpages.eyeo.com#$#hide-if-contains 'filter not applied' p[id]",
        "example.com#@$#abort-on-property-read window.open",
    ] {
        assert!(f(snippet).is_none(), "{:?} flagged as {:?}", snippet, f(snippet).map(|p| p.reason));
    }
    // A snippet argument may hold a brace of its own -- a regex quantifier --
    // without becoming CSS.
    assert!(f(r#"pluto.tv#$#replace-xhr-response /duration="PT[0-9]{1,2}[.][^"]*"/ '' 'x'"#).is_none());
    // AdGuard's `#$#` is CSS injection, not a snippet: it opens on a selector
    // and carries a declaration block, so it is still balance-checked --
    // including when the block was left unclosed.
    assert!(f("example.com#$#.ad { display: none !important; }").is_none());
    assert!(f("example.com#$#.ad[foo=\"bar { display: none !important; }").is_some());
    assert!(f("example.com#$#.ad { color: red").is_some());
    assert!(f("example.com#$#div { color: red").is_some());
}

#[test]
fn test_addheader_keeps_its_spaces() {
    use crate::fop_sort::filter_tidy;
    // Cookie attributes are space-separated; stripping them rewrites the
    // header the rule sets.
    let rule = "||crazyshit.com^$addheader=response:set-cookie:__trx1_p=c; path=/; max-age=21600";
    assert_eq!(filter_tidy(rule, false), rule, "addheader value was rewritten");
    // ...and the checker must not read those spaces as a defect, or
    // --remove-bad-rules deletes the rule filter_tidy just left alone.
    assert!(crate::fop_rules::check_rule(rule).is_none());
    // Recognised via KNOWN_OPTION_PREFIXES, which matches on the key of a
    // `key=value` option -- the bare name is not a valid option on its own.
    assert!(crate::is_known_option("addheader=response:set-cookie:x=c"));
    assert!(crate::is_known_option("removeheader=refresh"));
    // A value with no space takes the parsed path, where the option name is
    // checked against that list rather than skipped.
    assert!(crate::fop_rules::check_rule("||a.com^$addheader=response:x:y").is_none());
}

#[test]
fn test_regex_group_colon_is_not_a_pseudo_class() {
    use crate::fop_sort::element_tidy;
    let tidy = |sel: &str| element_tidy("a.com", "#?#", sel);
    // `(?:` and `(?i:` open regex groups, so the `:` is not a pseudo-class.
    // AdGuard's BaseFilter carries this rule, and lowercasing the group's
    // first alternative changed which text it matched.
    for kept in [
        "div:contains(/^(?:Reklama$|Dzieki)/)",
        "div:contains(/^(?i:ABC)/)",
        "div:contains(/^(?-is:ABC)/)",
        "div:contains(/(?:ABC$|DEF)/)",
    ] {
        assert_eq!(tidy(kept), format!("a.com#?#{}", kept), "case was changed");
    }
    // Real pseudo-classes are still lowercased, including when the walk back
    // over flag letters passes through a class name made of them.
    assert_eq!(tidy("div:HOVER"), "a.com#?#div:hover");
    assert_eq!(tidy(".mix:HOVER"), "a.com#?#.mix:hover");
    assert_eq!(tidy("li:NTH-CHILD(2)"), "a.com#?#li:nth-child(2)");
    // A selector carrying `:contains(` is extended syntax and is preserved
    // whole, so the trailing pseudo-class keeps its case too. That is the same
    // treatment `:has-text(` has always had.
    assert_eq!(
        tidy("div:contains(/^(?:ABC)/):HOVER"),
        "a.com#?#div:contains(/^(?:ABC)/):HOVER"
    );
}

#[test]
fn test_contains_argument_is_left_alone() {
    use crate::fop_sort::element_tidy;
    let tidy = |sel: &str| element_tidy("a.com", "#?#", sel);
    // `+` inside a `:contains()` argument is a regex quantifier or literal
    // text, not a sibling combinator. Padding it with spaces changed what
    // these AdGuard rules matched.
    for kept in [
        r".kevinsingle-content > p:contains(/^ +$/)",
        r".v-list-item:first-child:contains(/^ad\s+$/)",
        "div[class][aria-expanded=\"false\"]:contains(Reklama 18+.)",
        // And a bare `:` in the argument is not a pseudo-class.
        "div:contains(/foo:BAR/)",
    ] {
        assert_eq!(tidy(kept), format!("a.com#?#{}", kept), "argument was rewritten");
    }
}

#[test]
fn test_escaped_colon_is_not_a_pseudo_class() {
    use crate::fop_sort::element_tidy;
    let tidy = |sel: &str| element_tidy("a.com", "##", sel);
    // `\:` is a literal colon in an id, and ids are case-sensitive. AdGuard's
    // ChineseFilter carries `###js\:cookies\:barInitWrapper`, which fop was
    // rewriting to `barinitwrapper` -- a different id.
    for kept in [
        r"#js\:cookies\:barInitWrapper",
        r"#id\:KeepCase",
        r".cls\:KeepCase",
    ] {
        assert_eq!(tidy(kept), format!("a.com##{}", kept), "case was changed");
    }
    // An even run of backslashes escapes the backslash, not the colon, so what
    // follows really is a pseudo-class.
    assert_eq!(tidy(r"div\\:HOVER"), r"a.com##div\\:hover");
    // One of each in the same selector.
    assert_eq!(tidy(r"#esc\:Keep:HOVER"), r"a.com###esc\:Keep:hover");
}

#[test]
fn test_adguard_noop_modifier_survives() {
    use crate::fop_sort::filter_tidy;
    use crate::fop_rules::check_rule;
    // AdGuard's noop modifier is a run of underscores, used to keep a long
    // rule readable. The `_` -> `-` normalisation meant for option names like
    // `redirect_rule` was turning it into `-----`, which is not an option.
    for rule in [
        "*$script,third-party,denyallow=cdn.example.com,_____,domain=site.example",
        "||example.com^$script,__,domain=site.example",
        "||example.com^$_,third-party",
    ] {
        let tidied = filter_tidy(rule, false);
        let noop: String = rule
            .rsplit(['$', ','])
            .find(|t| !t.is_empty() && t.bytes().all(|b| b == b'_'))
            .expect("test rule carries a noop")
            .to_string();
        assert!(
            tidied.split(['$', ',']).any(|t| t == noop),
            "noop {:?} lost: {} -> {}", noop, rule, tidied
        );
        assert!(check_rule(rule).is_none(), "flagged: {}", rule);
    }
    // The normalisation it exists for still happens.
    assert!(filter_tidy("||example.com^$redirect_rule=noopjs", false).contains("redirect-rule="));
}

#[test]
fn test_matches_attr_is_extended() {
    use crate::fop_sort::element_tidy;
    // uBO's :matches-attr() takes literal/regex arguments, like the rest of
    // the matches-* family already in the extended list.
    for sel in [
        r#"div:matches-attr("/^data-.{4}$/"="/^v[45]{1}$/")"#,
        "div[class]:matches-attr(data-X=Y)",
    ] {
        assert_eq!(element_tidy("a.com", "##", sel), format!("a.com##{}", sel));
    }
}

#[test]
fn test_resolve_workers_precedence() {
    use crate::{resolve_workers, MAX_THREADS, MAX_WORKERS};
    // An explicit setting wins outright, and reports itself as the source.
    assert_eq!(resolve_workers(Some(2)), (2, "set"));
    assert_eq!(resolve_workers(Some(64)), (64, "set"));
    // With nothing set, the machine decides, held to the default cap.
    let (n, source) = resolve_workers(None);
    assert!((1..=MAX_WORKERS).contains(&n), "auto gave {}", n);
    // The environment is only credited when it was actually usable; this test
    // does not set it, so the source here is whatever the environment running
    // the suite provides.
    assert!(source == "auto" || source == "RAYON_NUM_THREADS");
    // The ceiling is a guard against a typo, not a limit on what may be asked.
    const { assert!(MAX_THREADS > MAX_WORKERS) };
    // ...and it guards every source, `.fopconfig` included, which reaches here
    // as an explicit setting.
    assert_eq!(resolve_workers(Some(99_999)), (MAX_THREADS, "set"));
}

// =============================================================================
// Review fixes since 5.5.0
// =============================================================================

#[test]
fn test_check_rule_never_flags_valid_modifiers() {
    use crate::fop_rules::check_rule as f;
    // Every one of these was flagged as a defect, so `--remove-bad-rules`
    // deleted it and `--ci` failed the build on it. 5.5.0 flagged none.
    for rule in [
        // Literal-argument pseudo-class: the apostrophe is text, not a quote.
        "example.com#?#div:-abp-contains(Don't miss)",
        "example.com#?#div:-abp-properties(content: \"(\")",
        // Bare modifiers that switch a whole class off, valid on exceptions.
        "@@||site.com^$urlblock",
        "@@||site.com^$removeheader",
        "@@||site.com^$replace",
        "@@||site.com^$redirect",
        "@@||site.com^$permissions",
        "@@||site.com^$dnsrewrite",
        // AdGuard DNS modifiers, and the strict-party spellings.
        "||site.com^$dnsrewrite=1.2.3.4",
        "||site.com^$dnstype=AAAA",
        "||site.com^$client=127.0.0.1",
        "||site.com^$ctag=device_phone",
        "||site.com^$strict-third-party",
        // An escaped comma is part of the value, not a second option.
        "||example.org^$permissions=sync-xhr=()\\,camera=()",
    ] {
        assert!(f(rule).is_none(), "valid rule flagged: {}", rule);
    }
    // The exception-only forms are still wrong on a blocking rule, where the
    // bare word is missing its value -- and a real typo is still caught.
    assert!(f("||site.com^$removeheader").is_some());
    assert!(f("||site.com^$urlblock").is_some());
    assert!(f("||site.com^$thrid-party").is_some());
}

#[test]
fn test_check_rule_html_filtering_rules() {
    use crate::fop_rules::check_rule as f;
    // AdGuard HTML filtering. These have no `#`, so they fell through to the
    // network path, where a bare tag parsed as an option list: both of the
    // first two are live in AdGuard Annoyances and were called unknown options.
    for rule in [
        "m.timesofindia.com,m-timesofindia-com.cdn.ampproject.org$$amp-consent",
        "portal.librus.pl$$advertisement-module",
        "example.com$@$amp-ad",
        "$$script[tag-content=\"ad config\"]",
        "example.com$$script[tag-content=\"x\"][max-length=\"500\"]",
        // A `##` inside the selector is not the separator: the earliest one is.
        "example.com$$div[attr=\"a##b\"]",
    ] {
        assert!(f(rule).is_none(), "valid rule flagged: {}", rule);
    }
    // Real faults in one are still caught, now as the cosmetic faults they are.
    assert_eq!(f("example.com$$div[id=\"ad\"").map(|p| p.reason), Some("unbalanced brackets in selector"));
    assert_eq!(f(",example.com$$div").map(|p| p.reason), Some("malformed domain list"));
    assert_eq!(f("example.com$$").map(|p| p.reason), Some("separator with no selector"));
    // A network rule whose path holds `$$` is still a network rule...
    assert_eq!(f("||a.com/$$p^$thrid-party").map(|p| p.reason), Some("unknown option"));
    // ...and a `##` rule whose selector holds `$$` is still a `##` rule.
    assert!(f("example.com##div[data-x=\"$$\"]").is_none());

    // In an HTML-filtering selector a backslash is text: AdGuard escapes a
    // quote by doubling it. Reading `\"` as an escape swallowed the closing
    // quote, and this valid rule was reported unbalanced and deleted.
    assert!(f("example.com$$script[tag-content=\"C:\\\"]").is_none());
    assert!(f("example.com$$script[tag-content=\"say \"\"hi\"\"\"]").is_none());
    // A regex inside :contains() keeps its escapes; that construct is exempt.
    assert!(f("youporn.com$$script:contains(/window\\.[\\s\\S]*?_zone_/)").is_none());
    // CSS does use backslash escapes, so a `##` selector still honours them:
    // the quote here is escaped, and the string closes after `b`.
    assert!(f("example.com##div[title=\"a\\\"b\"]").is_none());
}

#[test]
fn test_literal_arg_constructs_cover_text_matching_pseudos() {
    use crate::fop_rules::{check_rule, LITERAL_ARG_CONSTRUCTS};
    use crate::fop_sort::EXTENDED_PSEUDO;
    // The two lists drifted once: `:-abp-contains(` was in the sorter's list
    // and not the checker's, and rules using it were deleted. Every pseudo
    // whose argument is text must be treated as literal by the checker.
    let text_matching = |p: &str| {
        ["contains", "has-text", "matches-", "xpath", "properties", "watch-attr"]
            .iter()
            .any(|w| p.contains(w))
    };
    for pseudo in EXTENDED_PSEUDO.iter().filter(|p| text_matching(p)) {
        assert!(
            LITERAL_ARG_CONSTRUCTS.iter().any(|lit| pseudo.starts_with(lit)),
            "{} is text-matching but missing from LITERAL_ARG_CONSTRUCTS",
            pseudo
        );
        let rule = format!("example.com#?#div{}Don't miss)", pseudo);
        assert!(check_rule(&rule).is_none(), "flagged: {}", rule);
    }
}

#[test]
fn test_escaped_comma_survives_sorting() {
    use crate::fop_sort::split_unescaped_commas;
    assert_eq!(split_unescaped_commas("a,b\\,c,d"), vec!["a", "b\\,c", "d"]);
    // Backslashes are counted, as uBO counts them: an even run is escaped
    // backslashes before a real separator, an odd run escapes the comma
    assert_eq!(split_unescaped_commas(r"a\\,b"), vec![r"a\\", "b"]);
    assert_eq!(split_unescaped_commas(r"a\\\,b"), vec![r"a\\\,b"]);
    assert_eq!(split_unescaped_commas(r"a\\\\,b"), vec![r"a\\\\", "b"]);
    assert_eq!(split_unescaped_commas(r"\,a"), vec![r"\,a"]);
    assert_eq!(split_unescaped_commas(",a,"), vec!["", "a", ""]);
    assert_eq!(split_unescaped_commas("a"), vec!["a"]);
    assert_eq!(split_unescaped_commas(""), vec![""]);
    // The sorter split this in two, reordered the halves and left a dangling
    // backslash: `$camera=(),permissions=sync-xhr=()\`.
    let rule = "||example.org^$permissions=sync-xhr=()\\,camera=()";
    assert_eq!(filter_tidy(rule, false), rule);
    // Real separators still sort.
    assert_eq!(
        filter_tidy("||example.org^$third-party,permissions=a=()\\,b=(),script", false),
        "||example.org^$permissions=a=()\\,b=(),script,third-party"
    );
}

#[test]
fn test_has_text_trailing_letters_are_not_regex_flags() {
    use crate::fop_sort::combine_has_text_rules;
    // `/path/to` is plain text: `to` is not a set of JavaScript regex flags.
    // Reading it as flags merged to `/path|Sponsored/to`, which uBO cannot
    // compile and so matches as literal text -- that is, nothing.
    let merged = combine_has_text_rules(vec![
        "example.com##div:has-text(/path/to)".to_string(),
        "example.com##div:has-text(Sponsored)".to_string(),
    ]);
    assert_eq!(merged.len(), 1, "{:?}", merged);
    assert!(!merged[0].ends_with("/to)"), "text read as flags: {}", merged[0]);
    assert!(merged[0].contains("path/to"), "{}", merged[0]);
    // Real flags are still recognised, and that is now what stops the merge:
    // `Sponsored` is case-sensitive and `/Ad/i` is not, so there is no one
    // form to fold them into. Were `i` read as text rather than flags, the two
    // would both be plain and would merge -- so two rules out is the proof.
    let merged = combine_has_text_rules(vec![
        "example.com##div:has-text(/Ad/i)".to_string(),
        "example.com##div:has-text(Sponsored)".to_string(),
    ]);
    assert_eq!(
        merged,
        vec![
            "example.com##div:has-text(/Ad/i)".to_string(),
            "example.com##div:has-text(Sponsored)".to_string(),
        ]
    );
    // Agreeing on the flags, they still fold.
    assert_eq!(
        combine_has_text_rules(vec![
            "example.com##div:has-text(/Ad/i)".to_string(),
            "example.com##div:has-text(/Sponsored/i)".to_string(),
        ]),
        vec!["example.com##div:has-text(/Ad|Sponsored/i)".to_string()]
    );
    // A repeated flag is not a flag set JavaScript accepts, so it is text too.
    let dup = combine_has_text_rules(vec![
        "example.com##div:has-text(/a/ii)".to_string(),
        "example.com##div:has-text(b)".to_string(),
    ]);
    assert!(!dup[0].ends_with("/ii)"), "{:?}", dup);
}

/// A throwaway repository, removed when dropped.
struct ScratchRepo(std::path::PathBuf);

impl ScratchRepo {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir()
            .join(format!("fop-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = ScratchRepo(dir);
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "t@t"]);
        repo.git(&["config", "user.name", "t"]);
        repo
    }
    fn git(&self, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
    fn write(&self, file: &str, content: &str) {
        std::fs::write(self.0.join(file), content).unwrap();
    }
    fn cmd(&self) -> Vec<String> {
        vec!["git".into(), "-C".into(), self.0.display().to_string()]
    }
}

impl Drop for ScratchRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_added_lines_include_staged_rules() {
    let repo = ScratchRepo::new("staged");
    repo.write("a.txt", "||a.com^\n");
    repo.git(&["add", "a.txt"]);
    repo.git(&["commit", "-q", "-m", "one"]);
    repo.write("a.txt", "||a.com^\n||staged.com^\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "||a.com^\n||staged.com^\n||unstaged.com^\n");
    // `commit -a` carries both; a bare `git diff` showed only the unstaged one,
    // so a staged bad rule escaped every check and was committed.
    let added: Vec<String> = crate::fop_git::get_added_lines(&repo.cmd())
        .unwrap()
        .into_iter()
        .map(|a| a.content)
        .collect();
    assert!(added.contains(&"||staged.com^".to_string()), "{:?}", added);
    assert!(added.contains(&"||unstaged.com^".to_string()), "{:?}", added);
    assert!(!added.contains(&"||a.com^".to_string()), "{:?}", added);
}

#[test]
fn test_added_lines_on_an_unborn_branch() {
    let repo = ScratchRepo::new("unborn");
    repo.write("a.txt", "||first.com^\n");
    repo.git(&["add", "a.txt"]);
    let added = crate::fop_git::get_added_lines(&repo.cmd()).unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].content, "||first.com^");
}

#[test]
fn test_added_lines_on_an_unborn_sha256_branch() {
    // The empty tree has a different id under SHA-256; a hard-coded SHA-1 one
    // does not exist there, the diff failed, and the checks did not run.
    let dir = std::env::temp_dir().join(format!("fop-test-sha256-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let repo = ScratchRepo(dir);
    let init = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo.0)
        .args(["init", "-q", "--object-format=sha256"])
        .output()
        .unwrap();
    if !init.status.success() {
        // A git too old for SHA-256 repositories has nothing to test here.
        return;
    }
    repo.write("a.txt", "||first.com^\n");
    repo.git(&["add", "a.txt"]);
    let added = crate::fop_git::get_added_lines(&repo.cmd()).expect("diff failed");
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].content, "||first.com^");
}

#[test]
fn test_ci_diff_base_falls_back_without_a_default_branch() {
    // The shape of a shallow PR checkout: detached, no remote, two commits.
    let repo = ScratchRepo::new("nodefault");
    repo.write("a.txt", "||a.com^\n");
    repo.git(&["add", "a.txt"]);
    repo.git(&["commit", "-q", "-m", "one"]);
    repo.write("a.txt", "||a.com^\n||b.com^\n");
    repo.git(&["commit", "-q", "-am", "two"]);
    repo.git(&["checkout", "-q", "--detach"]);
    repo.git(&["branch", "-q", "-D", "main"]);
    // `get_default_branch(...)?` returned None here before the `HEAD~1`
    // fallback ran, and the audit failed the build.
    assert_eq!(crate::ci_diff_base(&repo.cmd()).as_deref(), Some("HEAD~1"));
}

#[test]
fn test_ci_diff_base_uses_the_fork_point() {
    let upstream = ScratchRepo::new("upstream");
    upstream.write("a.txt", "||keep.com^\n||deleted-upstream.com^\n");
    upstream.git(&["add", "a.txt"]);
    upstream.git(&["commit", "-q", "-m", "base"]);

    let clone = ScratchRepo(std::env::temp_dir()
        .join(format!("fop-test-clone-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&clone.0);
    let out = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(&upstream.0)
        .arg(&clone.0)
        .output()
        .unwrap();
    assert!(out.status.success());
    clone.git(&["config", "user.email", "t@t"]);
    clone.git(&["config", "user.name", "t"]);
    clone.git(&["checkout", "-q", "-b", "feature"]);
    clone.write("a.txt", "||keep.com^\n||deleted-upstream.com^\n||mine.com^\n");
    clone.git(&["commit", "-q", "-am", "mine"]);

    // Upstream moves on and deletes a rule the branch still carries.
    upstream.write("a.txt", "||keep.com^\n");
    upstream.git(&["commit", "-q", "-am", "delete"]);
    clone.git(&["fetch", "-q"]);

    let base = crate::ci_diff_base(&clone.cmd()).unwrap();
    let added: Vec<String> = crate::fop_git::get_added_lines_against(&clone.cmd(), Some(&base))
        .unwrap()
        .into_iter()
        .map(|a| a.content)
        .collect();
    // Against the tip, upstream's deletion showed up here as an addition.
    assert_eq!(added, vec!["||mine.com^".to_string()], "base {}", base);
}

// =============================================================================
// Rule checks run before the sort
// =============================================================================

fn test_sort_config(comment_chars: &[String]) -> crate::fop_sort::SortConfig<'_> {
    crate::fop_sort::SortConfig {
        convert_ubo: true,
        no_sort: false,
        alt_sort: false,
        abp_convert: false,
        adguard_convert: false,
        convert_trusted: false,
        parse_adguard: false,
        localhost: false,
        comment_chars,
        backup: false,
        keep_empty_lines: false,
        ignore_dot_domains: false,
        fix_typos: false,
        ignore_line_minimum: false,
        quiet: true,
        no_color: true,
        dry_run: false,
        output_changed: false,
        add_timestamp: false,
        benchmark: false,
    }
}

#[test]
fn test_tidy_rule_matches_the_sorter() {
    // `tidy_rule` mirrors the sorter's per-line dispatch so the checks can
    // judge a line as it will be written. If the two drift apart, the checks
    // judge a form nobody writes -- so every rule here goes through both.
    let chars = vec!["!".to_string()];
    let base = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-tidy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Under every option that changes a line, not just the defaults: the first
    // version of this test ran only those, and so missed that `tidy_rule` left
    // out the typo fixes the sort applies under `fix_typos`.
    let configs = [
        ("default", test_sort_config(&chars)),
        ("fix_typos", crate::fop_sort::SortConfig { fix_typos: true, ..test_sort_config(&chars) }),
        ("parse_adguard", crate::fop_sort::SortConfig { parse_adguard: true, ..test_sort_config(&chars) }),
        ("abp_convert", crate::fop_sort::SortConfig { abp_convert: true, ..test_sort_config(&chars) }),
        ("adguard_convert", crate::fop_sort::SortConfig { adguard_convert: true, ..test_sort_config(&chars) }),
        ("convert_trusted", crate::fop_sort::SortConfig { convert_trusted: true, ..test_sort_config(&chars) }),
        ("no_ubo_convert", crate::fop_sort::SortConfig { convert_ubo: false, ..test_sort_config(&chars) }),
    ];
    let _ = &base;
    for (name, config) in &configs {
    for (i, rule) in [
        "||x.com^$third-party.script",
        "||x.com^$redirect_rule=noopjs,xhr",
        "||x.com^$Third-Party,SCRIPT",
        "*$ping,third-party",
        "EXAMPLE.com##div  >  p",
        "b.com,a.com##.ad",
        "example.com#?#div:has-text(Sponsored)",
        "example.com#@#.banner",
        "/^\\w+\\.example\\.com$/##.ad",
        "example.com$$script[tag-content=\"ad\"]",
        "[$path=/x/]example.com##.ad",
        "@@||x.com^$generichide",
        "/ads/*",
        // Rewritten only under fix_typos: `$$domain=` becomes `$domain=`.
        "||y.com^$$domain=z.com",
        // Domain-list faults: element_tidy drops the empty entries under any
        // config; a doubled dot is repaired by nothing.
        "example..com##.ad",
        "a.com,,b.com##.ad",
        ",a.com##.ad",
        // Changed only by the conversions.
        "example.com##div:-abp-contains(Sponsored)",
        "example.com##div:has-text(Sponsored)",
        "example.com##+js(trusted-set-cookie, consent, true)",
        // Dropped domains are warned about; the warning must not escape.
        "a.b,good.com##.ad",
        // A hosts entry: the sorter keeps it as written, and the checks have
        // to judge that form, not the one a filter-list tidy would make of it.
        "0.0.0.0 example.com",
        "127.0.0.1 tracker.example.org",
        // Comments in either character, hash-space included: the sorter and
        // `tidy_rule` decide comment-ness separately, and this test missed the
        // hash-space case drifting between them until a line of it was added.
        "# a plain-text heading",
        "#",
        "#foo bar",
        "! a heading",
    ]
    .iter()
    .enumerate()
    {
        let file = dir.join(format!("{}-{}.txt", name, i));
        std::fs::write(&file, format!("{}\n", rule)).unwrap();
        crate::fop_sort::fop_sort(&file, config).unwrap();
        let sorted = std::fs::read_to_string(&file).unwrap();
        let written: Vec<&str> = sorted.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            written,
            vec![crate::fop_sort::tidy_rule(rule, config).as_ref()],
            "tidy_rule disagrees with the sorter on {} under {}",
            rule,
            name
        );
    }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_merge_key() {
    use crate::fop_rules::merge_key;
    // Rules the sort merges share a key...
    assert_eq!(merge_key("a.com##.ad"), merge_key("a.com,b.com##.ad"));
    assert_eq!(merge_key("||x.com^$script,domain=a.com"), merge_key("||x.com^$script,domain=a.com|b.com"));
    assert_eq!(merge_key("x.com##div:has-text(A)"), merge_key("x.com##div:has-text(/A|B/)"));
    assert_eq!(merge_key("a.com$$amp-ad"), merge_key("a.com,b.com$$amp-ad"));
    // Every text-matching pseudo-class the sort merges, not just :has-text().
    assert_eq!(merge_key("x.com##div:-abp-contains(A)"), merge_key("x.com,y.com##div:-abp-contains(/A|B/)"));
    assert_eq!(merge_key("x.com##div:abp-contains(A)"), merge_key("x.com##div:abp-contains(/A|B/)"));
    // ...and rules it would not merge do not.
    assert_ne!(merge_key("a.com##.ad"), merge_key("a.com##.banner"));
    assert_ne!(merge_key("a.com##.ad"), merge_key("a.com#@#.ad"));
}

fn rule_check_repo(name: &str, committed: &str, working: &str) -> ScratchRepo {
    let repo = ScratchRepo::new(name);
    repo.write("a.txt", committed);
    repo.git(&["add", "a.txt"]);
    repo.git(&["commit", "-q", "-m", "base"]);
    repo.write("a.txt", working);
    repo
}

fn run_checks(repo: &ScratchRepo) -> bool {
    let chars = vec!["!".to_string()];
    run_checks_with(repo, &|_: &std::path::Path| test_sort_config(&chars))
}

/// `run_checks`, with --remove-non-domain-on-add rather than --remove-bad-rules.
fn run_checks_non_domain(repo: &ScratchRepo) -> bool {
    let chars = vec!["!".to_string()];
    let config_for = |_: &std::path::Path| test_sort_config(&chars);
    crate::run_rule_checks(
        &repo.cmd(), false, true, None, false, &config_for, true,
        &["txt".to_string()], &[], &[], &[], false, false,
    )
}

fn run_checks_with<'c>(
    repo: &ScratchRepo,
    config_for: &(dyn Fn(&std::path::Path) -> crate::fop_sort::SortConfig<'c> + Sync),
) -> bool {
    crate::run_rule_checks(
        &repo.cmd(), true, false, None, false, config_for, true,
        &["txt".to_string()], &[], &[], &[], false, false,
    )
}

#[test]
fn test_remove_bad_rules_keeps_committed_rules() {
    // The reported case: the checks ran after the sort, which had merged the
    // added rule into the committed one, so both went.
    let repo = rule_check_repo("keep", "! t\na.com##.ad\n", "! t\na.com##.ad\nb..com##.ad\n");
    assert!(run_checks(&repo));
    assert_eq!(std::fs::read_to_string(repo.0.join("a.txt")).unwrap(), "! t\na.com##.ad\n");
}

#[test]
fn test_remove_bad_rules_holds_back_an_already_merged_line() {
    // A tree sorted earlier (a `--no-commit` run) already holds the merge, so
    // the line is the committed rule and the bad one together. Deleting it
    // would lose the committed rule: it is reported, kept, and the run fails
    // so nothing is committed.
    let merged = "! t\na.com,b..com##.ad\n";
    let repo = rule_check_repo("merged", "! t\na.com##.ad\n", merged);
    assert!(!run_checks(&repo));
    assert_eq!(std::fs::read_to_string(repo.0.join("a.txt")).unwrap(), merged);
}

#[test]
fn test_rule_checks_judge_the_repaired_form() {
    // Each of these is an "unknown option" -- a removable defect -- as written,
    // and fine once the sort has lowercased and normalised the option names.
    // Judging the raw line would delete three rules fop is about to repair.
    let working = "! t\n||a.com^\n||x.com^$redirect_rule=noopjs\n||y.com^$Third-Party\n||z.com^$SCRIPT,domain=a.com\n";
    let repo = rule_check_repo("repair", "! t\n||a.com^\n", working);
    assert!(run_checks(&repo));
    assert_eq!(std::fs::read_to_string(repo.0.join("a.txt")).unwrap(), working);
}

#[test]
fn test_webbundle_is_a_known_option() {
    assert!(crate::fop_rules::check_rule("||example.com^$webbundle").is_none());
}

#[test]
fn test_rule_checks_use_each_files_own_config() {
    // A file listed in `localhost_files` is sorted as a hosts file even when
    // the global setting is off. Judged under the global config, the space in
    // each added entry was stripped and the line reported as a bare domain.
    let repo = ScratchRepo::new("hosts");
    repo.write("hosts.txt", "0.0.0.0 a.com\n");
    repo.git(&["add", "hosts.txt"]);
    repo.git(&["commit", "-q", "-m", "base"]);
    let working = "0.0.0.0 a.com\n0.0.0.0 b.com\n";
    repo.write("hosts.txt", working);
    let chars = vec!["!".to_string()];
    let config_for = |path: &std::path::Path| crate::fop_sort::SortConfig {
        localhost: path.file_name().is_some_and(|n| n == "hosts.txt"),
        ..test_sort_config(&chars)
    };
    // Asserted on the findings themselves: the false one is advice, which is
    // kept, so the file's content alone could not show it was raised.
    let additions = crate::fop_git::get_added_lines(&repo.cmd()).unwrap();
    let tidied = crate::tidy_all(&additions, &config_for, &repo.0);
    let found: Vec<&str> = crate::check_as_sorted(&additions, &tidied)
        .iter()
        .map(|(_, p)| p.reason)
        .collect();
    assert!(found.is_empty(), "hosts entry judged under the global config: {:?}", found);
    assert!(run_checks_with(&repo, &config_for));
    assert_eq!(std::fs::read_to_string(repo.0.join("hosts.txt")).unwrap(), working);
}

#[test]
fn test_rule_checks_judge_repaired_domain_lists() {
    // element_tidy drops empty entries from a domain list, so a doubled,
    // leading or trailing comma is written repaired. Judged as typed, each is a
    // removable "malformed domain list" and would be deleted. A doubled dot is
    // repaired by nothing -- not by fix_typos either -- so it is still judged
    // and removed. Run under fix_typos, which leaves all of this unchanged.
    let working = "! t\na.com##.ad\nb.com,,c.com##.banner\n,d.com##.ad2\ne.com,##.ad3\n";
    let repo = rule_check_repo("typos", "! t\na.com##.ad\n", working);
    let chars = vec!["!".to_string()];
    let config_for =
        |_: &std::path::Path| crate::fop_sort::SortConfig { fix_typos: true, ..test_sort_config(&chars) };
    assert!(run_checks_with(&repo, &config_for));
    assert_eq!(std::fs::read_to_string(repo.0.join("a.txt")).unwrap(), working);

    // Something fix_typos does not repair is still judged, and removed.
    let repo = rule_check_repo("typos2", "! t\na.com##.ad\n", "! t\na.com##.ad\nexample..com##.x\n");
    assert!(run_checks_with(&repo, &config_for));
    assert_eq!(std::fs::read_to_string(repo.0.join("a.txt")).unwrap(), "! t\na.com##.ad\n");
}

#[test]
fn test_tidy_rule_is_silent() {
    // The sort warns when it writes the line; the checks' pass over the same
    // line must not, or every warning appears twice.
    use std::sync::atomic::Ordering;
    crate::WARNING_TO_FILE.store(true, Ordering::Relaxed);
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let marker = "silent-marker-4f1c.com";
    let rule = format!("a.b,{}##.ad", marker);
    let _ = crate::fop_sort::tidy_rule(&rule, &config);
    let leaked = crate::WARNING_BUFFER.lock().unwrap().iter().any(|w| w.contains(marker));
    // The same work done by the sorter's own path does warn, so the assertion
    // above is not passing merely because nothing warns.
    let _ = crate::fop_sort::element_tidy(&format!("a.b,{}", marker), "##", ".ad");
    let warned = crate::WARNING_BUFFER.lock().unwrap().iter().any(|w| w.contains(marker));
    assert!(!leaked, "tidy_rule printed a warning the sort will print again");
    assert!(warned, "element_tidy no longer warns, so this test proves nothing");
}

#[test]
fn test_empty_path_option_is_unset() {
    // An empty path-valued option means "not set", not the path "".
    use crate::non_empty_path;
    assert_eq!(non_empty_path(""), None);
    assert_eq!(non_empty_path("   "), None);
    assert_eq!(non_empty_path(" warnings.txt "), Some(std::path::PathBuf::from("warnings.txt")));
    assert_eq!(non_empty_path("/abs/banned.txt"), Some(std::path::PathBuf::from("/abs/banned.txt")));
}

#[test]
fn test_merge_guard_covers_abp_contains() {
    // The sort merges :-abp-contains() arguments as it does :has-text(); a
    // guard keyed only on :has-text( let this merged line be deleted, and the
    // committed x.com rule with it.
    let merged = "! t\nx.com,y..com##div:-abp-contains(/A|B/)\n";
    let repo = rule_check_repo("abpmerge", "! t\nx.com##div:-abp-contains(A)\n", merged);
    assert!(!run_checks(&repo));
    assert_eq!(std::fs::read_to_string(repo.0.join("a.txt")).unwrap(), merged);
}

#[test]
fn test_merge_guard_follows_a_rename() {
    // Renamed since HEAD, a file's committed rules live under its old name.
    // Taken as "not in HEAD, so nothing committed", its merged line was deleted.
    let repo = ScratchRepo::new("rename");
    let body = "! t\na.com##.ad\nc.com##.c\nd.com##.d\ne.com##.e\nf.com##.f\n";
    repo.write("old.txt", body);
    repo.git(&["add", "old.txt"]);
    repo.git(&["commit", "-q", "-m", "base"]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    let merged = "! t\na.com,b..com##.ad\nc.com##.c\nd.com##.d\ne.com##.e\nf.com##.f\n";
    repo.write("new.txt", merged);
    assert!(!run_checks(&repo));
    assert_eq!(std::fs::read_to_string(repo.0.join("new.txt")).unwrap(), merged);
}

#[test]
fn test_bad_rule_in_a_new_file_is_still_removed() {
    // A file confirmed absent from HEAD has no committed rules to lose, so the
    // guard must not hold its lines back.
    let repo = ScratchRepo::new("newfile");
    repo.write("a.txt", "! t\n");
    repo.git(&["add", "a.txt"]);
    repo.git(&["commit", "-q", "-m", "base"]);
    repo.write("fresh.txt", "! t\ngood.com##.ad\nbad..com##.ad\n");
    repo.git(&["add", "fresh.txt"]);
    assert!(run_checks(&repo));
    assert_eq!(std::fs::read_to_string(repo.0.join("fresh.txt")).unwrap(), "! t\ngood.com##.ad\n");
}

#[test]
fn test_file_at_head_classification() {
    use crate::fop_git::{file_at_head, AtHead};
    // Committed: its content.
    let repo = ScratchRepo::new("athead");
    repo.write("a.txt", "a.com##.ad\n");
    repo.git(&["add", "a.txt"]);
    repo.git(&["commit", "-q", "-m", "base"]);
    assert!(matches!(file_at_head(&repo.cmd(), "a.txt"), AtHead::Content(c) if c == "a.com##.ad\n"));
    // Confirmed absent from a real HEAD.
    assert!(matches!(file_at_head(&repo.cmd(), "never.txt"), AtHead::Absent));
    // An unborn branch has nothing committed.
    let unborn = ScratchRepo::new("athead-unborn");
    assert!(matches!(file_at_head(&unborn.cmd(), "a.txt"), AtHead::Absent));
    // Git that cannot run proves nothing either way: Unknown, never Absent,
    // or the merge guard would treat the file as having nothing to lose.
    let broken = vec!["/nonexistent/git-binary".to_string(), "-C".to_string(), repo.0.display().to_string()];
    assert!(matches!(file_at_head(&broken, "a.txt"), AtHead::Unknown));
}

// =============================================================================
// combine_filters: current behaviour, pinned
// =============================================================================
//
// These record what combine_filters does today -- quirks included -- so that
// any rewrite (it is O(n^2) in the size of a merge group) must reproduce it or
// change these deliberately. Expected values were taken from the current code,
// not written from a description of what it ought to do.

fn pinned(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_combine_filters_cosmetic_pinned() {
    use crate::fop_sort::combine_filters;
    let el = &*crate::ELEMENT_DOMAIN_PATTERN;
    for (name, input, expected) in [
        ("two", pinned(&["b.com##.ad", "a.com##.ad"]), pinned(&["a.com,b.com##.ad"])),
        ("chain", pinned(&["c.com##.ad", "a.com##.ad", "b.com##.ad"]), pinned(&["a.com,b.com,c.com##.ad"])),
        ("dedup", pinned(&["a.com,b.com##.ad", "b.com,c.com##.ad"]), pinned(&["a.com,b.com,c.com##.ad"])),
        ("different selector", pinned(&["a.com##.ad", "b.com##.banner"]), pinned(&["a.com##.ad", "b.com##.banner"])),
        // Only adjacent rules merge; the caller sorts them together first.
        ("not adjacent", pinned(&["a.com##.ad", "b.com##.banner", "c.com##.ad"]), pinned(&["a.com##.ad", "b.com##.banner", "c.com##.ad"])),
        ("both exclusions", pinned(&["~a.com##.ad", "~b.com##.ad"]), pinned(&["~a.com,~b.com##.ad"])),
        // Exclusions-only never merges with a list that includes.
        ("include vs exclude", pinned(&["a.com##.ad", "~b.com##.ad"]), pinned(&["a.com##.ad", "~b.com##.ad"])),
        // ...but the test is "only exclusions", so a list holding one of each
        // counts as including, and merges with a plain list.
        ("mixed within one", pinned(&["a.com,~x.a.com##.ad", "b.com##.ad"]), pinned(&["a.com,b.com,~x.a.com##.ad"])),
        // A generic rule has no domains to merge, on either side.
        ("generic first", pinned(&["##.ad", "a.com##.ad"]), pinned(&["##.ad", "a.com##.ad"])),
        ("generic last", pinned(&["a.com##.ad", "##.ad"]), pinned(&["a.com##.ad", "##.ad"])),
        // Ordered by base domain, the excluded form after the included one.
        ("tilde order", pinned(&["b.com,~a.com##.ad", "a.com##.ad"]), pinned(&["a.com,~a.com,b.com##.ad"])),
        ("exception", pinned(&["b.com#@#.ad", "a.com#@#.ad"]), pinned(&["a.com,b.com#@#.ad"])),
        ("extended", pinned(&["b.com#?#div:has(> .ad)", "a.com#?#div:has(> .ad)"]), pinned(&["a.com,b.com#?#div:has(> .ad)"])),
        ("different separator", pinned(&["a.com##.ad", "b.com#@#.ad"]), pinned(&["a.com##.ad", "b.com#@#.ad"])),
        // Case-sensitive ordering: the sort lowercases cosmetic domains before
        // this point, but the function itself does not.
        ("case", pinned(&["B.com##.ad", "a.com##.ad"]), pinned(&["B.com,a.com##.ad"])),
        ("single", pinned(&["a.com##.ad"]), pinned(&["a.com##.ad"])),
        ("empty", pinned(&[]), pinned(&[])),
    ] {
        assert_eq!(combine_filters(input, el, ","), expected, "cosmetic case: {}", name);
    }
}

#[test]
fn test_combine_filters_network_pinned() {
    use crate::fop_sort::combine_filters;
    let net = &*crate::FILTER_DOMAIN_PATTERN;
    for (name, input, expected) in [
        ("two", pinned(&["||x^$script,domain=b.com", "||x^$script,domain=a.com"]), pinned(&["||x^$script,domain=a.com|b.com"])),
        ("different options", pinned(&["||x^$script,domain=a.com", "||x^$image,domain=b.com"]), pinned(&["||x^$script,domain=a.com", "||x^$image,domain=b.com"])),
        ("domain first", pinned(&["||x^$domain=b.com,script", "||x^$domain=a.com,script"]), pinned(&["||x^$domain=a.com|b.com,script"])),
        ("both exclusions", pinned(&["||x^$domain=~b.com", "||x^$domain=~a.com"]), pinned(&["||x^$domain=~a.com|~b.com"])),
        ("include vs exclude", pinned(&["||x^$domain=a.com", "||x^$domain=~b.com"]), pinned(&["||x^$domain=a.com", "||x^$domain=~b.com"])),
        // `$` in the replacement is escaped, so a pattern holding one survives.
        ("dollar in pattern", pinned(&["/ads\\$x/$domain=b.com", "/ads\\$x/$domain=a.com"]), pinned(&["/ads\\$x/$domain=a.com|b.com"])),
        ("no domain option", pinned(&["||x^$script", "||x^$script,domain=a.com"]), pinned(&["||x^$script", "||x^$script,domain=a.com"])),
        ("chain with a list", pinned(&["||x^$domain=c.com", "||x^$domain=a.com|b.com", "||x^$domain=d.com"]), pinned(&["||x^$domain=a.com|b.com|c.com|d.com"])),
    ] {
        assert_eq!(combine_filters(input, net, "|"), expected, "network case: {}", name);
    }
}

#[test]
fn test_combine_filters_records_each_pairwise_step() {
    // Merge steps are recorded for the PR description pairwise: merging three
    // rules records two steps, the second holding the first's result.
    use crate::fop_sort::{combine_filters_recorded, CombineRecord};
    let el = &*crate::ELEMENT_DOMAIN_PATTERN;
    let record_of = |group: Vec<String>, room: usize| {
        let mut record = CombineRecord { steps: Vec::new(), room, count: 0 };
        let out = combine_filters_recorded(group, el, ",", &mut record);
        (out, record)
    };
    let (out, record) = record_of(pinned(&["c.pin7.test##.step", "a.pin7.test##.step", "b.pin7.test##.step"]), 40);
    assert_eq!(out, pinned(&["a.pin7.test,b.pin7.test,c.pin7.test##.step"]));
    assert_eq!(
        record.steps,
        vec![
            (pinned(&["c.pin7.test##.step", "a.pin7.test##.step"]), "a.pin7.test,c.pin7.test##.step".to_string()),
            (pinned(&["a.pin7.test,c.pin7.test##.step", "b.pin7.test##.step"]), "a.pin7.test,b.pin7.test,c.pin7.test##.step".to_string()),
        ]
    );
    assert_eq!(record.count, 2);

    // A large group: one line and 49 steps, of which only as many as the PR
    // description will list are kept in full; the rest are counted.
    let group: Vec<String> = (0..50).map(|i| format!("d{:02}.pin8.test##.big", 49 - i)).collect();
    let domains: Vec<String> = (0..50).map(|i| format!("d{:02}.pin8.test", i)).collect();
    let (full_out, full) = record_of(group.clone(), usize::MAX);
    assert_eq!(full_out, vec![format!("{}##.big", domains.join(","))]);
    assert_eq!((full.steps.len(), full.count), (49, 49));
    for room in [0, 1, 5, 40, 48, 49] {
        let (out, record) = record_of(group.clone(), room);
        assert_eq!(out, full_out, "room {}", room);
        assert_eq!(record.count, 49, "room {}", room);
        // The first steps, exactly as the uncapped record has them
        assert_eq!(record.steps[..], full.steps[..room], "room {}", room);
    }
}

/// Held by the tests that read or replace FOP's global state -- the change
/// record, the warning buffer -- so one cannot see another's writes.
static GLOBAL_STATE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_combine_filters_tracking_is_capped() {
    // Through the global record, as --pr-show-changes uses it. Other tests
    // may merge while tracking is on, but together they can only add steps:
    // the list stops at what the description shows, and the count at least
    // holds this group's 49.
    use crate::fop_sort::{combine_filters, PR_CHANGES_SHOWN, SORT_CHANGES, TRACK_CHANGES};
    use std::sync::atomic::Ordering::Relaxed;
    let _guard = GLOBAL_STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let el = &*crate::ELEMENT_DOMAIN_PATTERN;
    TRACK_CHANGES.store(true, Relaxed);
    let group: Vec<String> = (0..50).map(|i| format!("d{:02}.pin9.test##.cap", 49 - i)).collect();
    let out = combine_filters(group, el, ",");
    TRACK_CHANGES.store(false, Relaxed);
    let domains: Vec<String> = (0..50).map(|i| format!("d{:02}.pin9.test", i)).collect();
    assert_eq!(out, vec![format!("{}##.cap", domains.join(","))]);
    let changes = SORT_CHANGES.lock().unwrap();
    assert_eq!(changes.domains_combined.len(), PR_CHANGES_SHOWN);
    assert!(changes.domains_combined_count >= 49, "{}", changes.domains_combined_count);
}

#[test]
fn test_pr_changes_counts_unlisted_merges() {
    // The description lists the recorded steps and counts the rest from the
    // total, since only the listed ones are kept.
    use crate::fop_sort::{SortChanges, SORT_CHANGES};
    let _guard = GLOBAL_STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let body = {
        let mut changes = SORT_CHANGES.lock().unwrap();
        let saved = std::mem::take(&mut *changes);
        *changes = SortChanges {
            domains_combined: (0..40).map(|i| (pinned(&["a##.x", "b##.x"]), format!("r{}", i))).collect(),
            domains_combined_count: 45,
            ..SortChanges::default()
        };
        drop(changes);
        let body = crate::fop_git::format_pr_changes();
        *SORT_CHANGES.lock().unwrap() = saved;
        body
    };
    assert!(body.contains("## Domains Combined"), "{}", body);
    assert!(body.contains("- ... and 5 more"), "{}", body);
}

/// combine_filters as it stood before the linear rewrite, verbatim apart
/// from dropping the change record: the oracle both merge paths are held to.
#[allow(clippy::all)]
fn combine_filters_reference(
    mut uncombined: Vec<String>,
    domain_pattern: &regex::Regex,
    separator: &str,
) -> Vec<String> {
    // No combining needed for single filter
    if uncombined.len() <= 1 {
        return uncombined;
    }
    let mut combined: Vec<String> = Vec::with_capacity(uncombined.len());

    for i in 0..uncombined.len() {
        let domains1 = domain_pattern.captures(&uncombined[i]);

        // Get domain info for current and next filter
        let (domain1_str, domains1_full) = if i + 1 < uncombined.len() {
            if let Some(ref caps) = domains1 {
                (
                    caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                    caps.get(0).map(|m| m.as_str()).unwrap_or(""),
                )
            } else {
                ("", "")
            }
        } else {
            ("", "")
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

        let domain2_str = domains2
            .as_ref()
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");

        if domain2_str.is_empty() {
            combined.push(std::mem::take(&mut uncombined[i]));
            continue;
        }

        let domains2_full = domains2
            .as_ref()
            .and_then(|c| c.get(0))
            .map(|m| m.as_str())
            .unwrap_or("");

        // Check if domain patterns are compatible (same structure except domain list)
        let pattern1_with_domain2 = domains1_full.replace(domain1_str, domain2_str);
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
            .collect::<ahash::AHashSet<_>>()
            .into_iter()
            .collect();

        new_domains
            .sort_unstable_by(|a, b| {
                let (a_base, a_inv) = a.strip_prefix('~').map(|s| (s, true)).unwrap_or((a.as_str(), false));
                let (b_base, b_inv) = b.strip_prefix('~').map(|s| (s, true)).unwrap_or((b.as_str(), false));
                (a_base, a_inv).cmp(&(b_base, b_inv))
            });

        let new_domain_str = new_domains.join(separator);

        // Create the substitution pattern (full match with new domains)
        let domains_substitute = domains1_full.replace(domain1_str, &new_domain_str);

        // Escape $ for regex replacement ($ is special in replacement strings)
        let escaped_substitute = if domains_substitute.contains('$') {
            domains_substitute.replace("$", "$$")
        } else {
            domains_substitute
        };

        // Modify the next filter to be the combined version
        // (using filter i as the base, replacing its domain pattern with the combined domains)

        let combined_filter = domain_pattern
            .replace(&uncombined[i], escaped_substitute.as_str())
            .to_string();


        uncombined[i + 1] = combined_filter;

        // Don't add current filter to combined - it will be processed as part of next iteration
    }

    combined
}

/// Deterministic xorshift, so a failing case reproduces.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

/// A domain list of 1-4 entries, mostly ordinary domains, sometimes the
/// shapes that send a rule down the pairwise path: `#`, `$`, `=` and the
/// other separator bytes, repeated `~`, an entry that is also option text,
/// and empty entries.
fn fuzz_domains(rng: &mut Xorshift, separator: &str) -> String {
    const ORDINARY: [&str; 9] = ["a.com", "b.com", "c.org", "d.net", "a.com", "~a.com", "~b.com", "~c.org", "x"];
    const AWKWARD: [&str; 11] = ["~~a.com", "a.com#", "a=b", "", "a$b", "~", "party", "script", "a?b", "a%b", "a@b"];
    let n = 1 + rng.next() % 4;
    (0..n)
        .map(|_| if rng.next().is_multiple_of(8) { rng.pick(&AWKWARD) } else { rng.pick(&ORDINARY) })
        .collect::<Vec<_>>()
        .join(separator)
}

fn fuzz_element_rule(rng: &mut Xorshift) -> String {
    const SEPARATORS: [&str; 8] = ["##", "##", "#@#", "#?#", "#$#", "#@?#", "$$", "$@$"];
    const SELECTORS: [&str; 4] = [".ad", ".ad", "#banner", ".x > .y"];
    let domains = if rng.next().is_multiple_of(8) { String::new() } else { fuzz_domains(rng, ",") };
    format!("{}{}{}", domains, rng.pick(&SEPARATORS), rng.pick(&SELECTORS))
}

fn fuzz_network_rule(rng: &mut Xorshift) -> String {
    const PATTERNS: [&str; 3] = ["||x.com^", "||x.com^", "/a$/"];
    const OPTIONS: [&str; 7] = ["script", "third-party", "csp=a.com", "csp=a|b", "image", "domain=c.org", "~third-party"];
    let mut options: Vec<String> = Vec::new();
    for _ in 0..rng.next() % 3 {
        options.push(rng.pick(&OPTIONS).to_string());
    }
    if !rng.next().is_multiple_of(6) {
        let at = (rng.next() % (options.len() as u64 + 1)) as usize;
        options.insert(at, format!("domain={}", fuzz_domains(rng, "|")));
    }
    format!("{}${}", rng.pick(&PATTERNS), options.join(","))
}

#[test]
fn test_combine_filters_linear_matches_reference() {
    use crate::fop_sort::{combine_filters_linear, combine_filters_recorded, CombineRecord};
    type FuzzCase<'a> = (&'a regex::Regex, &'a str, fn(&mut Xorshift) -> String);
    let patterns: [FuzzCase; 3] = [
        (&crate::ELEMENT_DOMAIN_PATTERN, ",", fuzz_element_rule),
        (&crate::ADGUARD_ELEMENT_DOMAIN_PATTERN, ",", fuzz_element_rule),
        (&crate::FILTER_DOMAIN_PATTERN, "|", fuzz_network_rule),
    ];
    // Not one fop uses: its lead-in ends in an ordinary byte, so a domain
    // list of `x`s can be found overlapping it. Holds the linear path to
    // combine_pair for patterns beyond today's three.
    let overlapping = regex::Regex::new(r"\$d:x([^,]+)").unwrap();
    let overlap_rule: fn(&mut Xorshift) -> String = |rng| {
        const DOMAINS: [&str; 5] = ["xx", "x", "xa.com", "a.com", "xxx"];
        let n = 1 + rng.next() % 3;
        let domains: Vec<&str> = (0..n).map(|_| rng.pick(&DOMAINS)).collect();
        format!("r$d:x{}", domains.join(","))
    };
    // Nor this: its tail holds an ordinary byte, so a domain `x` is also
    // found inside it
    let tailing = regex::Regex::new(r"^([^#]*?)x##").unwrap();
    let tail_rule: fn(&mut Xorshift) -> String = |rng| {
        const DOMAINS: [&str; 4] = ["x", "a.com", "xa", "b.com"];
        let n = 1 + rng.next() % 3;
        let domains: Vec<&str> = (0..n).map(|_| rng.pick(&DOMAINS)).collect();
        format!("{}x##.ad", domains.join(","))
    };
    let mut rng = Xorshift(0x9e37_79b9_7f4a_7c15);
    let mut merged_groups = 0;
    let made_up = [(&overlapping, ",", overlap_rule), (&tailing, ",", tail_rule)];
    for (pattern, separator, rule) in patterns.into_iter().chain(made_up) {
        for case in 0..5_000 {
            let n = 1 + (rng.next() % 10) as usize;
            let mut group: Vec<String> = (0..n).map(|_| rule(&mut rng)).collect();
            // Half the groups sorted as the sorter would, so runs form
            if case % 2 == 0 {
                group.sort_by_cached_key(|s| pattern.replace(s, "").into_owned());
            }
            let expected = combine_filters_reference(group.clone(), pattern, separator);
            merged_groups += usize::from(expected.len() < group.len());
            assert_eq!(combine_filters_linear(group.clone(), pattern, separator), expected, "linear: {:?}", group);
            // The tracked path, recording some or all of its steps
            let room = [0, 1, 2, usize::MAX][case % 4];
            let mut record = CombineRecord { steps: Vec::new(), room, count: 0 };
            let recorded = combine_filters_recorded(group.clone(), pattern, separator, &mut record);
            assert_eq!(recorded, expected, "recorded, room {}: {:?}", room, group);
            assert_eq!(record.count, group.len() - expected.len(), "count, room {}: {:?}", room, group);
            assert_eq!(record.steps.len(), room.min(record.count), "steps, room {}: {:?}", room, group);
        }
    }
    // The fuzz must actually exercise merging, not just pass through
    assert!(merged_groups > 2_000, "only {} groups merged", merged_groups);
}

#[test]
fn test_combine_filters_linear_large_group() {
    // One selector across thousands of domains, arriving in many rules with
    // overlaps: the case the linear path exists for
    use crate::fop_sort::combine_filters_linear;
    let mut rng = Xorshift(42);
    for (pattern, separator, shape) in [
        (&*crate::ELEMENT_DOMAIN_PATTERN, ",", "{}##.ad"),
        (&*crate::FILTER_DOMAIN_PATTERN, "|", "||x.com^$script,domain={}"),
    ] {
        let group: Vec<String> = (0..120)
            .map(|_| {
                let domains: Vec<String> = (0..1 + rng.next() % 30)
                    .map(|_| {
                        let d = rng.next() % 1500;
                        if d.is_multiple_of(7) { format!("~d{}.test", d) } else { format!("d{}.test", d) }
                    })
                    .collect();
                shape.replace("{}", &domains.join(separator))
            })
            .collect();
        let expected = combine_filters_reference(group.clone(), pattern, separator);
        // Rules with and without exclusions only merge among themselves,
        // so a handful of lines, not one
        assert!(expected.len() < 10, "{} lines", expected.len());
        assert_eq!(combine_filters_linear(group, pattern, separator), expected);
    }
}

#[test]
fn test_sort_timestamp_keeps_rules_with_the_text() {
    // With add-timestamp on, the sort refreshes timestamp lines in the first
    // lines of every section. Rules carrying the text used to be replaced by
    // a `! Last updated:` comment and lost; only the header comment updates.
    let chars = vec!["!".to_string()];
    let mut config = test_sort_config(&chars);
    config.add_timestamp = true;
    let dir = std::env::temp_dir().join(format!("fop-test-timestamp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("list.txt");
    let rules = [
        "##.x:-abp-contains(Last modified:)",
        "example.com##div:has-text(Last updated:)",
        "||example.com/last-updated:^",
    ];
    std::fs::write(&file, format!(
        "! Title: pin\n! Last modified: 1 Jan 2020 00:00 UTC\n{}\n! Section\n{}\n",
        rules.join("\n"), rules.join("\n"))).unwrap();
    crate::fop_sort::fop_sort(&file, &config).unwrap();
    let result = std::fs::read_to_string(&file).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let lines: Vec<&str> = result.lines().collect();
    assert!(lines[1].starts_with("! Last modified: ") && !lines[1].contains("2020"), "{:?}", lines[1]);
    for rule in rules {
        assert_eq!(lines.iter().filter(|l| **l == rule).count(), 2, "{} lost: {:?}", rule, lines);
    }
    assert_eq!(lines.iter().filter(|l| is_timestamp_line(l)).count(), 1, "{:?}", lines);
}

#[test]
fn test_sort_keeps_adguard_hint_targets() {
    // An AdGuard hint applies to the line after it. That rule is a section of
    // its own -- processed like any rule, but not sorted or merged -- while
    // the rest of its section is sorted as usual. Sorting used to move another rule under the hint: 30
    // of AdGuard's 2,515 moved, an iOS-only exception among them.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-hints-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("list.txt");
    std::fs::write(&file, [
        "! Title: pin",
        // Sorting would put a.com first; merging would fold b.com in
        "!+ PLATFORM(ios)", "z.com##.x", "b.com##.ad", "a.com##.ad",
        // A chain of hints targets the first rule after it, which is not
        // merged with its twin below
        "!+ NOT_OPTIMIZED", "!+ PLATFORM(ios)", "b.com##.ad", "a.com##.ad",
        // A blank line does not end the hint
        "!+ PLATFORM(ios)", "", "z.com##.y", "a.com##.y",
        // Any other comment does
        "!+ PLATFORM(ios)", "! plain", "z.com##.w", "a.com##.w",
        // The target is still tidied
        "!+ PLATFORM(ios)", "||a.com^$xhr,3p", "||0.com^",
        // and still checked like any rule: one removed as TLD-only leaves the
        // hint over whichever rule now follows it, which stays put in turn
        "!+ PLATFORM(ios)", ".com", "z.com##.v", "a.com##.v",
    ].join("\n") + "\n").unwrap();
    crate::fop_sort::fop_sort(&file, &config).unwrap();
    let result = std::fs::read_to_string(&file).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(result.lines().collect::<Vec<_>>(), vec![
        "! Title: pin",
        "!+ PLATFORM(ios)", "z.com##.x", "a.com,b.com##.ad",
        "!+ NOT_OPTIMIZED", "!+ PLATFORM(ios)", "b.com##.ad", "a.com##.ad",
        "!+ PLATFORM(ios)", "z.com##.y", "a.com##.y",
        "!+ PLATFORM(ios)", "! plain", "a.com,z.com##.w",
        "!+ PLATFORM(ios)", "||a.com^$third-party,xmlhttprequest", "||0.com^",
        "!+ PLATFORM(ios)", "z.com##.v", "a.com##.v",
    ]);
}

#[test]
fn test_scriptlet_escaped_comma_kept() {
    // `\,` is a comma inside a uBO scriptlet argument. The spacing fix split
    // on every comma (since 825e1cf, v4.3.2), so an escaped one gained a
    // space and the argument changed: a cookie value, several regexes and
    // needles. Every such rule in the uBO snapshots, each already spaced as
    // fop writes it, so each must come back unchanged.
    let chars = vec!["!".to_string()];
    let mut config = test_sort_config(&chars);
    for scriptlet in [
        r"+js(nostif, ()\,a\,b);, 5000)",
        r"+js(aeld, /^load[A-Za-z]{12\,}/)",
        r"+js(nostif, )](this\,..., 3000-6000)",
        r"+js(rpnt, script, /adv_pre_src.*\,/)",
        r"+js(trusted-set-cookie, Cookie, accept_cookies\,\,, , , reload, 1)",
        r"+js(rpnt, script, /data: \[.*\]\,/, data: []\,, condition, ads_num)",
        r"+js(nostif, /^\s*function\s*\(\s*\)\s*{\s*[a-zA-Z]{1\,2}\s*\(.{1\,10}$/)",
        r"+js(m3u-prune, /\,ad\n.+?(?=#UPLYNK-SEGMENT)/gm, /uplynk\.com\/.*?\.m3u8/)",
        r"+js(nostif, /function\(\)\s*\{\s*var .{70\,300}\s*\)\s*\}\s*$/, 4000-6000)",
        r"+js(trusted-set-local-storage-item, cookieConsent, necessary\,preferences)",
        r"+js(m3u-prune, /#EXT-X-DISCONTINUITY.{1\,100}#EXT-X-DISCONTINUITY/gm, mixed.m3u8)",
        r"+js(trusted-set-attr, ins.adsbygoogle.nitro-side\,ins.adsbygoogle.nitro-banner, data-ad-status, filled)",
        r"+js(trusted-click-element, .kw-ads-pagination-button:first-child\,.kw-ads-pagination-button:first-child, , 1000)",
        r"+js(no-xhr-if, /\/api\/stats\/atr\?.+?&rt=\d+\.\d+.+?&volume=\d+&cbr=.+?&fexp=v1%[-%0-9C]{300\,}&.+?&muted=\d(&vis=3)?&docid=/ method:POST)",
        r"+js(rpnt, script, /  function [a-zA-Z]{1\,2}\([a-zA-Z]{1\,2}\,[a-zA-Z]{1\,2}\).*?\(\)\{return [a-zA-Z]{1\,2}\;\}\;return [a-zA-Z]{1\,2}\(\)\;\}/)",
        // An escaped comma followed by a space is still one argument
        r"+js(trusted-click-element, #CybotCookiebotDialogBodyLevelButtonStatisticsInline\, #CybotCookiebotDialogBodyLevelButtonMarketingInline\, #CybotCookiebotDialogBodyLevelButtonLevelOptinAllowallSelection)",
    ] {
        let rule = format!("example.com##{}", scriptlet);
        assert_eq!(crate::fop_sort::tidy_rule(&rule, &config), rule);
    }

    // Spacing is still normalised at the commas that separate arguments
    for (rule, expected) in [
        (r"example.com##+js(set,a\,b ,1)", r"example.com##+js(set, a\,b, 1)"),
        (r"example.com##+js(set ,a\,b,  1)", r"example.com##+js(set, a\,b, 1)"),
        // `\\,` is an escaped backslash, then a real separator
        (r"example.com##+js(set,a\\,b)", r"example.com##+js(set, a\\, b)"),
        // An empty argument stays one
        (r"example.com##+js(set,,1)", r"example.com##+js(set, , 1)"),
    ] {
        assert_eq!(crate::fop_sort::tidy_rule(rule, &config), expected);
    }

    // A quoted argument may hold a comma, so rules with any of uBO's three
    // quote characters are left as written
    for rule in [
        r#"example.com##+js(set,a,"x,y")"#,
        "example.com##+js(set,a,'x,y')",
        "example.com##+js(set,a,`x,y`)",
    ] {
        assert_eq!(crate::fop_sort::tidy_rule(rule, &config), rule);
    }

    // --convert-trusted splits the same way: `a\, true` is one argument, the
    // cookie's name, with no value to vouch for; and a value followed by
    // further arguments is not a lone safe value
    config.convert_trusted = true;
    for rule in [r"example.com##+js(trusted-set-cookie, a\, true)", "example.com##+js(trusted-set-cookie, a, 1, x)"] {
        assert_eq!(crate::fop_sort::tidy_rule(rule, &config), rule);
    }
    assert_eq!(
        crate::fop_sort::tidy_rule("example.com##+js(trusted-set-cookie, a, true)", &config),
        "example.com##+js(set-cookie, a, true)"
    );
    // An escaped backslash does not hide a separator
    assert_eq!(
        crate::fop_sort::tidy_rule(r"example.com##+js(trusted-set-cookie, a\\, true)", &config),
        r"example.com##+js(set-cookie, a\\, true)"
    );
}

#[test]
fn test_benchmark_sort_leaves_the_file() {
    // --benchmark times the sort alone: no comparison or diff (which on a
    // heavily reordered list took far longer than the sort), no result, and
    // the file and its directory exactly as they were.
    let chars = vec!["!".to_string()];
    let config = crate::fop_sort::SortConfig { benchmark: true, dry_run: true, ..test_sort_config(&chars) };
    let dir = std::env::temp_dir().join(format!("fop-test-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("list.txt");
    let content = "! Title: t\n||b.com^\n||a.com^\nb.com##.ad\na.com##.ad\n";
    std::fs::write(&file, content).unwrap();
    let result = crate::fop_sort::fop_sort(&file, &config).unwrap();
    let after = std::fs::read_to_string(&file).unwrap();
    let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(result, None);
    assert_eq!(after, content);
    assert_eq!(entries, vec![std::ffi::OsString::from("list.txt")], "temp file left behind");
}

#[test]
fn test_remote_moved_on() {
    // A push that lost the race with another push is retried after a rebase;
    // anything else is shown as git reported it. The first message is the
    // one GitHub gave easylist when two pushes crossed.
    use crate::fop_git::remote_moved_on;
    for raced in [
        " ! [remote rejected]         master -> master (cannot lock ref 'refs/heads/master': is at 0168719f39364c18d68b886ef2963f86474a347d but expected 0cc160a18091b05ebe5eb8151d062d9f63cb3a4d)\nerror: failed to push some refs to 'github.com:easylist/easylist.git'",
        " ! [rejected]        master -> master (fetch first)\nerror: failed to push some refs to 'origin'\nhint: Updates were rejected because the remote contains work that you do not\nhint: have locally.",
        " ! [rejected]        master -> master (non-fast-forward)\nerror: failed to push some refs to 'origin'",
        " ! [remote rejected] master -> master (incorrect old value provided)\nerror: failed to push some refs to 'origin'",
    ] {
        assert!(remote_moved_on(raced), "{}", raced);
    }
    for other in [
        "remote: Permission to easylist/easylist.git denied to someone.\nfatal: unable to access 'https://github.com/easylist/easylist.git/': The requested URL returned error: 403",
        "fatal: The current branch topic has no upstream branch.",
        " ! [remote rejected] master -> master (protected branch hook declined)\nerror: failed to push some refs to 'origin'",
        "",
    ] {
        assert!(!remote_moved_on(other), "{}", other);
    }
}

/// A bare remote and a clone of it, both removed on drop.
fn remote_and_clone(name: &str) -> (ScratchRepo, ScratchRepo) {
    let remote = ScratchRepo::new(&format!("{}-remote", name));
    std::fs::remove_dir_all(&remote.0).unwrap();
    std::fs::create_dir_all(&remote.0).unwrap();
    remote.git(&["init", "-q", "--bare", "-b", "main"]);
    let seed = ScratchRepo::new(&format!("{}-seed", name));
    seed.write("list.txt", "! Title: t\n||a.com^\n||b.com^\n");
    seed.write("other.txt", "x\n");
    seed.git(&["add", "."]);
    seed.git(&["commit", "-qm", "init"]);
    seed.git(&["push", "-q", &remote.0.display().to_string(), "main"]);
    let clone = ScratchRepo::new(&format!("{}-clone", name));
    std::fs::remove_dir_all(&clone.0).unwrap();
    let out = std::process::Command::new("git")
        .args(["clone", "-q"]).arg(&remote.0).arg(&clone.0).output().unwrap();
    assert!(out.status.success());
    clone.git(&["config", "user.email", "t@t"]);
    clone.git(&["config", "user.name", "t"]);
    (remote, clone)
}

/// Another clone pushes `file` with `content` to the remote.
fn push_from_elsewhere(remote: &ScratchRepo, name: &str, file: &str, content: &str) {
    let other = ScratchRepo::new(name);
    std::fs::remove_dir_all(&other.0).unwrap();
    let out = std::process::Command::new("git")
        .args(["clone", "-q"]).arg(&remote.0).arg(&other.0).output().unwrap();
    assert!(out.status.success());
    other.git(&["config", "user.email", "t@t"]);
    other.git(&["config", "user.name", "t"]);
    other.write(file, content);
    other.git(&["commit", "-qam", "elsewhere"]);
    other.git(&["push", "-q", "origin", "main"]);
}

#[test]
fn test_pull_and_push_stops_at_a_conflict() {
    // The pull after committing hit a conflict: a rebase is left in progress
    // with HEAD detached. It used to push regardless, failing in a cascade of
    // errors that never said "conflict", and exit 0. It must stop, push
    // nothing, and leave the commit to be finished by hand.
    use crate::fop_git::{pull_and_push, PushOutcome, GIT};
    let (remote, clone) = remote_and_clone("pp-conflict");
    clone.write("list.txt", "! Title: t\n||a.com^\n||b.com^\n||c.com^\n");
    clone.git(&["commit", "-qam", "mine"]);
    push_from_elsewhere(&remote, "pp-conflict-other", "list.txt", "! Title: t\n||a.com^\n||b.com^\n||z.org^\n");
    let before = remote.git(&["rev-parse", "main"]);

    assert!(matches!(pull_and_push(&clone.cmd(), &GIT, true, true), PushOutcome::Stopped));
    assert_eq!(remote.git(&["rev-parse", "main"]), before, "something was pushed");
    assert!(!clone.git(&["diff", "--name-only", "--diff-filter=U"]).is_empty(), "no conflict left to resolve");
    // The commit is intact, waiting on the rebase
    assert!(clone.git(&["log", "--all", "--format=%s"]).lines().any(|s| s == "mine"));
}

#[test]
fn test_pull_and_push_rebases_onto_a_clean_change() {
    // The usual case: someone pushed an unrelated change. The pull rebases
    // over it and the push lands on top.
    use crate::fop_git::{pull_and_push, PushOutcome, GIT};
    let (remote, clone) = remote_and_clone("pp-clean");
    clone.write("list.txt", "! Title: t\n||a.com^\n||b.com^\n||c.com^\n");
    clone.git(&["commit", "-qam", "mine"]);
    push_from_elsewhere(&remote, "pp-clean-other", "other.txt", "y\n");

    assert!(matches!(pull_and_push(&clone.cmd(), &GIT, true, true), PushOutcome::Pushed));
    assert_eq!(remote.git(&["log", "--format=%s", "main"]).lines().collect::<Vec<_>>(), ["mine", "elsewhere", "init"]);
}

#[test]
fn test_repo_config_may_not_run_or_write_outside() {
    // A .fopconfig in the working directory may be a pull request's: it may
    // not choose the program run as git, nor aim a write outside the tree.
    use crate::{restrict_repo_config, stays_in_tree_of};
    use std::path::{Path, PathBuf};
    let base = std::env::temp_dir().join(format!("fop-test-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("repo");
    std::fs::create_dir_all(tree.join("logs")).unwrap();
    std::fs::create_dir_all(base.join("elsewhere")).unwrap();
    for inside in ["warn.txt", "logs/warn.txt", "./warn.txt"] {
        assert!(stays_in_tree_of(Path::new(inside), &tree), "{}", inside);
    }
    for outside in ["/etc/passwd", "../warn.txt", "logs/../../x", "missing/warn.txt"] {
        assert!(!stays_in_tree_of(Path::new(outside), &tree), "{}", outside);
    }
    // A folder along the way that is a symlink out of the tree: the text
    // looks relative, but the file would land elsewhere
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(base.join("elsewhere"), tree.join("out")).unwrap();
        assert!(!stays_in_tree_of(Path::new("out/planted.txt"), &tree));
    }
    let _ = std::fs::remove_dir_all(&base);

    // Set by the repository's config: git-binary and an absolute
    // warning-output dropped, a relative warning-output kept
    let (mut bin, mut warn) = (Some("./evil.sh".to_string()), Some(PathBuf::from("/home/u/.bashrc")));
    let from_config = (bin.clone(), warn.clone(), None);
    assert!(restrict_repo_config(&mut bin, &mut warn, &None, from_config).is_ok());
    assert_eq!((bin, warn), (None, None));
    let mut warn = Some(PathBuf::from("warn.txt"));
    let from_config = (None, warn.clone(), None);
    assert!(restrict_repo_config(&mut None, &mut warn, &None, from_config).is_ok());
    assert_eq!(warn, Some(PathBuf::from("warn.txt")));

    // Replaced on the command line: the user's choice, not judged
    let (mut bin, mut warn) = (Some("/usr/bin/git".to_string()), Some(PathBuf::from("/tmp/w.txt")));
    let from_config = (Some("./evil.sh".to_string()), Some(PathBuf::from("/home/u/.bashrc")), None);
    assert!(restrict_repo_config(&mut bin, &mut warn, &None, from_config).is_ok());
    assert_eq!((bin.as_deref(), warn.as_deref()), (Some("/usr/bin/git"), Some(std::path::Path::new("/tmp/w.txt"))));

    // output-diff outside the tree is fatal rather than dropped, since
    // dropping it would sort instead of only reporting; unless replaced
    let outside = Some(PathBuf::from("/home/u/.bashrc"));
    assert!(restrict_repo_config(&mut None, &mut None, &outside, (None, None, outside.clone())).is_err());
    let cli = Some(PathBuf::from("out.diff"));
    assert!(restrict_repo_config(&mut None, &mut None, &cli, (None, None, outside)).is_ok());
}

#[cfg(unix)]
#[test]
fn test_no_write_through_symlinks() {
    // Every file FOP creates beside a list has a predictable name, so a
    // repository could plant a symlink there aimed at a file elsewhere.
    use crate::fop_sort::{create_file_no_follow, write_file_no_follow};
    let dir = std::env::temp_dir().join(format!("fop-test-nofollow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let victim = dir.join("victim.txt");
    std::fs::write(&victim, "precious").unwrap();

    let planted = dir.join("list.temp");
    std::os::unix::fs::symlink(&victim, &planted).unwrap();
    assert!(create_file_no_follow(&planted).is_err());
    assert!(write_file_no_follow(&planted, b"sorted").is_err());
    // A dangling link is refused too, rather than creating its target
    let dangling = dir.join("list.backup");
    std::os::unix::fs::symlink(dir.join("nowhere.txt"), &dangling).unwrap();
    assert!(write_file_no_follow(&dangling, b"sorted").is_err());
    assert!(!dir.join("nowhere.txt").exists());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");

    // A stale regular file is replaced; a new one created; a directory refused
    let stale = dir.join("old.temp");
    std::fs::write(&stale, "stale and longer").unwrap();
    write_file_no_follow(&stale, b"new").unwrap();
    assert_eq!(std::fs::read_to_string(&stale).unwrap(), "new");
    write_file_no_follow(&dir.join("fresh.diff"), b"diff").unwrap();
    assert!(create_file_no_follow(&dir).is_err());

    // Through the sort: a planted `.temp` or `.backup` link leaves its target
    // alone, and the list is left unsorted rather than written without them
    let chars = vec!["!".to_string()];
    let config = crate::fop_sort::SortConfig { backup: true, ..test_sort_config(&chars) };
    for planted in ["list.temp", "list.backup"] {
        let _ = std::fs::remove_file(dir.join("list.temp"));
        let _ = std::fs::remove_file(dir.join("list.backup"));
        std::os::unix::fs::symlink(&victim, dir.join(planted)).unwrap();
        let list = dir.join("list.txt");
        std::fs::write(&list, "! t\n||b.com^\n||a.com^\n").unwrap();
        let _ = crate::fop_sort::fop_sort(&list, &config);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious", "{}", planted);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn test_list_symlinks_stay_in_tree() {
    // A list that links out of the tree would have FOP read a file from
    // elsewhere and commit its contents; links within the tree are fine.
    use crate::{canonical_root, list_file_in_tree};
    let base = std::env::temp_dir().join(format!("fop-test-intree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("repo");
    std::fs::create_dir_all(tree.join("sub")).unwrap();
    std::fs::write(tree.join("sub/real.txt"), "! t\n").unwrap();
    std::fs::write(base.join("secret.txt"), "secret\n").unwrap();
    std::os::unix::fs::symlink("sub/real.txt", tree.join("inside.txt")).unwrap();
    std::os::unix::fs::symlink(base.join("secret.txt"), tree.join("outside.txt")).unwrap();
    std::os::unix::fs::symlink("../secret.txt", tree.join("climb.txt")).unwrap();
    std::os::unix::fs::symlink("gone.txt", tree.join("dangling.txt")).unwrap();
    let root = canonical_root(&tree);

    assert!(list_file_in_tree(&tree.join("sub/real.txt"), &root));
    assert!(list_file_in_tree(&tree.join("inside.txt"), &root));
    for escaping in ["outside.txt", "climb.txt", "dangling.txt"] {
        assert!(!list_file_in_tree(&tree.join(escaping), &root), "{}", escaping);
    }
    assert!(!list_file_in_tree(&tree.join("sub"), &root), "a directory is not a list");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn test_html_filter_is_not_read_as_options() {
    // `$$` and `$@$` open an AdGuard HTML filter: what follows is a selector,
    // not an option list. Read as options in the default mode, FOP warned that
    // `amp-consent` in `...$$amp-consent` was unknown -- on a rule it was
    // right to leave alone, and in a list it has no way to convert.
    use crate::{WARNING_BUFFER, WARNING_TO_FILE};
    use std::sync::atomic::Ordering::Relaxed;
    let _guard = GLOBAL_STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let warned_about = |fragment: &str| {
        WARNING_BUFFER.lock().unwrap().iter().any(|w| w.contains(fragment))
    };
    WARNING_TO_FILE.store(true, Relaxed);
    WARNING_BUFFER.lock().unwrap().clear();

    // Rules that carry a `$` but are cosmetic: HTML filters, AdGuard CSS
    // injection, a scriptlet argument, a regex in :has-text()
    let untouched = [
        "m.timesofindia.com,m-timesofindia-com.cdn.ampproject.org$$amp-consent",
        "portal.librus.pl$$advertisement-module",
        "example.com$@$script[tag-content=\"x\"]",
        "example.com$$div[id=\"a\"][class=\"b,c\"]",
        "example.com#$#body { background: url(\"a$b\"); }",
        "example.com#%#//scriptlet('set-constant', 'a$b', 'true')",
        "example.com#?#div:has-text(/a$b/)",
    ];
    let rewritten: Vec<&str> = untouched
        .iter()
        .filter(|rule| crate::fop_sort::filter_tidy(rule, true) != **rule)
        .copied()
        .collect();
    // A network rule's unknown option is still worth saying
    crate::fop_sort::filter_tidy("||example.com^$scirpt", true);
    let (selector_warnings, option_warning) = (
        warned_about("amp-consent") || warned_about("advertisement-module"),
        warned_about("scirpt"),
    );
    // Restored before asserting, so a failure here does not leave every later
    // test's warnings buffered instead of printed
    WARNING_TO_FILE.store(false, Relaxed);
    WARNING_BUFFER.lock().unwrap().clear();

    assert!(rewritten.is_empty(), "rewritten: {:?}", rewritten);
    assert!(!selector_warnings, "warned about a selector");
    assert!(option_warning, "an unknown option on a network rule must still warn");
}

#[test]
fn test_element_prefilter_keeps_every_separator() {
    // The element patterns are only consulted for a line that could carry a
    // cosmetic separator, since they are the expensive kind -- a lazy
    // `([^/|@"!]*?)` and three capture groups puts the regex crate on its
    // backtracking and PikeVM engines, which a profile of a real sort shows as
    // its two hottest functions. Every separator holds a `#` bar AdGuard's
    // `$$` and `$@$`, so those are what the prefilter must not lose: dropping
    // the AdGuard half of it leaves 162 lines of test-lists sorted wrongly and
    // no test red, which is why this one exists.
    let chars = vec!["!".to_string()];
    let adguard = crate::fop_sort::SortConfig { parse_adguard: true, ..test_sort_config(&chars) };
    // Sorted and lowercased domains prove the rule went down the cosmetic
    // path; left as written proves it did not.
    assert_eq!(
        crate::fop_sort::tidy_rule("B.com,a.com$$amp-ad", &adguard),
        "a.com,b.com$$amp-ad"
    );
    assert_eq!(
        crate::fop_sort::tidy_rule("D.com,c.com$@$script[tag-content=\"x\"]", &adguard),
        "c.com,d.com$@$script[tag-content=\"x\"]"
    );
    // Without the mode they are not cosmetic, and are left alone.
    let plain = test_sort_config(&chars);
    assert_eq!(
        crate::fop_sort::tidy_rule("B.com,a.com$$amp-ad", &plain),
        "B.com,a.com$$amp-ad"
    );
    // The `#` separators go through the prefilter in every mode.
    for (rule, want) in [
        ("B.com,a.com##.ad", "a.com,b.com##.ad"),
        ("B.com,a.com#@#.ad", "a.com,b.com#@#.ad"),
        ("B.com,a.com#?#div:has(> .ad)", "a.com,b.com#?#div:has(> .ad)"),
        ("B.com,a.com#$?#div", "a.com,b.com#$?#div"),
    ] {
        assert_eq!(crate::fop_sort::tidy_rule(rule, &adguard), want, "{rule}");
    }
    // A line with neither character cannot be cosmetic, and is untouched.
    assert_eq!(crate::fop_sort::tidy_rule("||plain.com^", &adguard), "||plain.com^");
}

#[test]
fn test_remove_non_domain_on_add() {
    // A bare word is a legal rule -- it blocks any URL containing it, and
    // `fingerprintjs` and `pkaystream` are real ones -- so nothing removes it
    // by default. It is also what a stray paste looks like: the line that
    // prompted this was `isCookiesAccepted`, one argument of a scriptlet,
    // sorted quietly into easylist because no check objected. The flag is for
    // a list whose author knows they do not write that shape.
    let repo = rule_check_repo(
        "non-domain-add",
        // Committed: two real bare-word rules, which the checks never see,
        // since they judge additions alone.
        "[Adblock Plus 2.0]\n! T\nfingerprintjs\npkaystream\n\
         5sim.net,aerolineas.com.ar##+js(set-local-storage-item, isCookiesAccepted, true)\n",
        // Added: a stray word, a substring rule wearing its boundary markers,
        // a domain, and a real rule that merges into the committed one.
        "[Adblock Plus 2.0]\n! T\nfingerprintjs\npkaystream\n\
         5sim.net,aerolineas.com.ar##+js(set-local-storage-item, isCookiesAccepted, true)\n\
         isCookiesAccepted\n-120x600-\n_social-button-\nexample.com\n\
         cellcom.co.il##+js(set-local-storage-item, isCookiesAccepted, true)\n",
    );
    run_checks_non_domain(&repo);
    let after = std::fs::read_to_string(repo.0.join("a.txt")).unwrap();
    let has = |s: &str| after.lines().any(|l| l == s);

    // The stray word goes.
    assert!(!has("isCookiesAccepted"), "the bare word survived:\n{after}");
    // A boundary marker says substring rule, so those stay however bare.
    assert!(has("-120x600-"), "an edge-marked substring was removed:\n{after}");
    assert!(has("_social-button-"), "an edge-marked substring was removed:\n{after}");
    // A domain is not a bare word, and is advice at most.
    assert!(has("example.com"), "a domain was removed:\n{after}");
    // The committed bare words are not additions, so they are never judged.
    assert!(has("fingerprintjs"), "a committed bare word was removed:\n{after}");
    assert!(has("pkaystream"), "a committed bare word was removed:\n{after}");
    // And the real rule added alongside it is kept. The checks run before the
    // sort, so it is still its own line here; merging it into the committed
    // twin is the sort's job and is covered by the merge tests.
    assert!(
        after.lines().any(|l| l.starts_with("cellcom.co.il##+js(")),
        "the added rule was removed:\n{after}"
    );
}

#[test]
fn test_narrow_flag_does_not_fail_on_defects_it_kept() {
    // The re-read after removing asks whether anything flagged is still
    // there. It used to count every defect, so a run given only
    // --remove-non-domain-on-add reported the `##` it had deliberately kept
    // as "could not be removed" -- contradicting the line above saying it was
    // kept -- and returned false, which in interactive mode stops the commit.
    // It must ask what this run was asked to remove, not what is a defect.
    let repo = rule_check_repo(
        "narrow-flag-keeps",
        "[Adblock Plus 2.0]\n! T\nzoho.com##.ad\n",
        "[Adblock Plus 2.0]\n! T\nisCookiesAccepted\n##\n||x.com^$fakeopt\nzoho.com##.ad\n",
    );
    let chars = vec!["!".to_string()];
    let config_for = |_: &std::path::Path| test_sort_config(&chars);
    // Interactive, which is where the false return actually stopped a commit.
    let ok = crate::run_rule_checks(
        &repo.cmd(), false, true, None, false, &config_for, true,
        &["txt".to_string()], &[], &[], &[], false, true,
    );
    let after = std::fs::read_to_string(repo.0.join("a.txt")).unwrap();
    assert!(ok, "the run failed over defects it was never asked to remove:\n{after}");
    // The bare word went; the defects outside the flag stayed.
    assert!(!after.lines().any(|l| l == "isCookiesAccepted"), "{after}");
    assert!(after.lines().any(|l| l == "##"), "a defect outside the flag was removed:\n{after}");
    assert!(after.lines().any(|l| l == "||x.com^$fakeopt"), "{after}");

    // With --remove-bad-rules as well, all three go and the run still passes.
    let repo = rule_check_repo(
        "narrow-flag-both",
        "[Adblock Plus 2.0]\n! T\nzoho.com##.ad\n",
        "[Adblock Plus 2.0]\n! T\nisCookiesAccepted\n##\n||x.com^$fakeopt\nzoho.com##.ad\n",
    );
    let ok = crate::run_rule_checks(
        &repo.cmd(), true, true, None, false, &config_for, true,
        &["txt".to_string()], &[], &[], &[], false, true,
    );
    let after = std::fs::read_to_string(repo.0.join("a.txt")).unwrap();
    assert!(ok, "the run failed with both flags:\n{after}");
    for gone in ["isCookiesAccepted", "##", "||x.com^$fakeopt"] {
        assert!(!after.lines().any(|l| l == gone), "{gone} survived both flags:\n{after}");
    }
}

#[test]
fn test_banned_list_is_not_checked_as_a_filter_list() {
    // The banned-domain list is a registry of names: bare entries are what
    // belongs there, and easylist's holds two without a dot. Checking it as a
    // filter list would see --remove-non-domain-on-add delete them. The path
    // is known from --check-banned-list, so it is skipped whether or not
    // `ignorefiles` also names it.
    let repo = ScratchRepo::new("banned-not-checked");
    repo.write("banned.txt", "example.com\nfingerprintjs\n");
    repo.write("a.txt", "[Adblock Plus 2.0]\n! T\nzoho.com##.ad\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "base"]);
    // A bare entry added to each: one belongs, the other does not.
    repo.write("banned.txt", "example.com\nfingerprintjs\npkaystream\n");
    repo.write("a.txt", "[Adblock Plus 2.0]\n! T\nisCookiesAccepted\nzoho.com##.ad\n");

    let chars = vec!["!".to_string()];
    let config_for = |_: &std::path::Path| test_sort_config(&chars);
    crate::run_rule_checks(
        &repo.cmd(), false, true, Some("banned.txt"), false, &config_for, true,
        &["txt".to_string()], &[], &[], &[], false, false,
    );
    let banned = std::fs::read_to_string(repo.0.join("banned.txt")).unwrap();
    let list = std::fs::read_to_string(repo.0.join("a.txt")).unwrap();
    assert!(banned.lines().any(|l| l == "pkaystream"), "a banned-list entry was removed:\n{banned}");
    assert!(!list.lines().any(|l| l == "isCookiesAccepted"), "the filter list was not checked:\n{list}");
}

#[test]
fn test_non_domain_words_are_kept_without_the_flag() {
    // The default is unchanged: nothing removes a bare word, and no check
    // even mentions one. 112 distinct bare-word rules live across easylist,
    // uAssets, AdguardFilters and test-lists, so this is the behaviour that
    // must not drift.
    let repo = rule_check_repo(
        "non-domain-default",
        "[Adblock Plus 2.0]\n! T\nzoho.com##.ad\n",
        "[Adblock Plus 2.0]\n! T\nisCookiesAccepted\nzoho.com##.ad\n",
    );
    run_checks(&repo);
    assert!(
        std::fs::read_to_string(repo.0.join("a.txt")).unwrap().lines().any(|l| l == "isCookiesAccepted"),
        "--remove-bad-rules removed a bare word"
    );
    assert!(
        crate::fop_rules::check_rule("isCookiesAccepted").is_none(),
        "a bare word is flagged without the flag"
    );
    // The predicate itself, on the shapes that decide it.
    use crate::fop_rules::is_non_domain_word as w;
    for yes in ["isCookiesAccepted", "fingerprintjs", "page_view_count", "728x90px", "a1"] {
        assert!(w(yes), "{yes} should qualify");
    }
    for no in ["-120x600-", "_social-button-", "-ads", "ads_", "example.com", "", "a"] {
        assert!(!w(no), "{no} should not qualify");
    }
}

#[test]
fn test_extract_leading_host() {
    // Replaces `^\|*([^/\^\$]+)`, which ran -- with a capture group built for
    // each -- on 70% of network rules. Compared against that regex over 2.6M
    // lines of four corpora, the two part company on one line only, noted
    // below.
    use crate::fop_sort::extract_leading_host as h;
    assert_eq!(h("||example.com^$script"), Some(("example.com", 13)));
    assert_eq!(h("||example.com"), Some(("example.com", 13)));
    assert_eq!(h("|http://example.com/a"), Some(("http:", 6)));
    assert_eq!(h("||com/*/ModalEngage|"), Some(("com", 5)));
    assert_eq!(h("||cfd^"), Some(("cfd", 5)));
    assert_eq!(h("example.com^"), Some(("example.com", 11)));
    assert_eq!(h("||chamsocthe-$doc"), Some(("chamsocthe-", 13)));
    // Nothing before the first delimiter, so no host.
    assert_eq!(h(""), None);
    assert_eq!(h("||"), None);
    assert_eq!(h("^abc"), None);
    assert_eq!(h("$script"), None);
    // The one disagreement: the regex backtracks, `\|*` yielding its `|` so
    // the group can take it, and calls the host `|`. The scan says there is no
    // host. Nothing downstream can tell -- a host of `|` is followed by a
    // path, so neither form warns -- and "no host" is the truer answer.
    assert_eq!(h("|/nbsys3/fsyspp.js"), None);
}

#[test]
fn test_no_dot_warning_only_where_a_typo_could_hide() {
    // The mention is for a domain that might be mistyped, so it is worth only
    // a pattern that is nothing but the host. A path or wildcard under the
    // host, a host prefix ending in `-`, and a host left to `ipaddress=` were
    // each built deliberately -- nobody mistypes a domain and then writes a
    // path beneath it -- and warning on them buried the rest: 109 mentions on
    // uAssets alone, 278 over four corpora, not one a typo. Every rule here is
    // real. The rules themselves are untouched either way; only the mention
    // goes, so this pins the noise, not the output.
    use crate::{WARNING_BUFFER, WARNING_TO_FILE};
    use std::sync::atomic::Ordering::Relaxed;
    let _guard = GLOBAL_STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-nodotwarn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let quiet = [
        "||com/*/ModalEngage|$script,third-party",
        "||de/trck/eclick/*^url=http$document,urlskip=?url",
        "||chamsocthe-$document",
        "||re-captha-version-$all",
        "||cc^$document,ipaddress=15.207.81.128",
        "||com^*.php?*&r=&p=&g=|$document",
        // An address is not a domain that forgot its dots. IPv4 is held back
        // by the dot test itself, which is why the regex that used to check it
        // here could go; IPv6 by the `[`.
        "||1.2.3.4^$script",
        "||[::1]^$third-party",
        "||[fe80::1]^$script",
    ];
    let loud = [
        "||cfd^$popup,third-party,domain=multiup.io",
        "||appcodepnik^",
        "||undefined^$script,redirect=noopjs",
        "||xhamster$document,replace=/popunder//",
    ];

    WARNING_TO_FILE.store(true, Relaxed);
    WARNING_BUFFER.lock().unwrap().clear();
    let file = dir.join("list.txt");
    std::fs::write(
        &file,
        format!("! Title: pin\n{}\n{}\n", quiet.join("\n"), loud.join("\n")),
    )
    .unwrap();
    crate::fop_sort::fop_sort(&file, &config).unwrap();
    let mentioned: Vec<String> = WARNING_BUFFER
        .lock()
        .unwrap()
        .iter()
        .filter(|w| w.contains("no dot in its domain"))
        .cloned()
        .collect();
    let sorted = std::fs::read_to_string(&file).unwrap();
    // Restored before asserting, so a failure does not leave every later
    // test's warnings buffered instead of printed.
    WARNING_TO_FILE.store(false, Relaxed);
    WARNING_BUFFER.lock().unwrap().clear();

    for rule in quiet {
        assert!(
            !mentioned.iter().any(|w| w.contains(rule)),
            "mentioned a shape no typo can take: {rule}\n{mentioned:#?}"
        );
    }
    for rule in loud {
        assert!(
            mentioned.iter().any(|w| w.contains(rule)),
            "a bare no-dot host went unmentioned: {rule}\n{mentioned:#?}"
        );
    }
    // Quiet or loud, the rule is written back either way -- as the sort tidies
    // it, which for `||undefined^$script,redirect=noopjs` means sorted options.
    for rule in quiet.iter().chain(loud.iter()) {
        let written = crate::fop_sort::tidy_rule(rule, &config);
        assert!(sorted.lines().any(|l| l == written), "dropped: {rule}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_rules_without_a_dot_are_kept() {
    // A domain with no dot is a whole-TLD or prefix match, not a mistake FOP
    // can tell: `||cfd^$popup,domain=multiup.io` blocks an abuse TLD,
    // `||countly-` a host prefix, `||com/services/?rt=` a path under any .com.
    // Deleting them dropped 11 rules from AdguardFilters and 11 from uAssets.
    // A TLD-only pattern, which matches every host under it, still goes.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-nodot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let kept = [
        "||cfd^$popup,third-party,domain=multiup.io",
        "||countly-",
        "||com/services/?rt=$script,third-party",
        "||tech^$app=com.imo.android.imoim",
        "|/nbsys3/fsyspp.js",
    ];
    let removed = [".com", "||.net^"];
    let sort = |config: &crate::fop_sort::SortConfig| {
        let file = dir.join("list.txt");
        std::fs::write(&file, format!("! Title: pin
{}
{}
||keep.example^
", kept.join("
"), removed.join("
"))).unwrap();
        crate::fop_sort::fop_sort(&file, config).unwrap();
        std::fs::read_to_string(&file).unwrap()
    };
    for config in [&config, &crate::fop_sort::SortConfig { ignore_dot_domains: true, ..test_sort_config(&chars) }] {
        let result = sort(config);
        for rule in kept {
            assert!(result.lines().any(|l| l == rule), "{} was dropped", rule);
        }
        for rule in removed {
            assert!(!result.lines().any(|l| l == rule), "{} was kept", rule);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_space_valued_options_keep_their_spaces() {
    // `carries_space_valued_option` holds back the whitespace strip. It listed
    // `csp=` and `header=` but not these four, so a space in their values was
    // run out: uAssets' badware.txt carried five `reason=` rules whose English
    // was welded together (`reason="Blatant scammers who are not related"`).
    // The other three take a regex, where a space is part of what matches --
    // `ipaddress=/^1\.2\.3\.4 $/` stripped is a different address -- and none
    // carries one in the lists today, which is why this pins them.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    for rule in [
        "||x.com^$all,reason=\"Blatant scammers who are not related\"",
        "||y.com^$all,reason=no quotes here",
        "||a.com^$uritransform=/a b/c d/",
        "||b.com^$urltransform=/x y/z/",
        "||c.com^$doc,ipaddress=/^1\\.2\\.3\\.4 $/",
    ] {
        assert_eq!(crate::fop_sort::tidy_rule(rule, &config), rule, "space stripped");
    }
    // A rule whose option value holds no space is tidied as before: the guard
    // must key on the option, not merely on the rule carrying one of these.
    assert_eq!(
        crate::fop_sort::tidy_rule("||d.com^$doc,reason=onetwo", &config),
        "||d.com^$document,reason=onetwo"
    );
    // `top=` restricts to the top-level context, as `to=` does the
    // destination. uBO added it in 2026; FOP called it an unknown option on
    // two rules in uAssets' filters-general.txt.
    assert!(
        crate::fop_rules::check_rule("||e.com^$script,3p,to=com,top=pro|to|~gov.to").is_none(),
        "top= was judged unknown"
    );
    assert_eq!(
        crate::fop_rules::check_rule("||f.com^$script,tpo=x.com").unwrap().reason,
        "unknown option",
        "a near-miss on it should still be caught"
    );
}

#[test]
fn test_hash_space_lines_are_comments() {
    // Plain URL registries that ship beside filter lists comment with `#`:
    // uAssets' badlists.txt is one. Such a line matches no cosmetic separator
    // -- those are all two characters -- so it used to sort as a network rule
    // and lose its spaces, `# Reek's Anti-Adblock Killer` becoming
    // `#Reek'sAnti-AdblockKiller` and floating away from the URLs it labelled.
    // The whitespace is what makes it a comment, and it must introduce
    // something: `#foo` is left alone, and a lone `#` introduces nothing, so
    // it is left to the line-length minimum that removes any other
    // one-character line. A lone `!` keeps its pass, being the comment
    // character itself and a section spacer 6,886 times over in the corpora,
    // where a lone `#` does not appear once.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-hash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("list.txt");
    std::fs::write(
        &file,
        "! Title: pin\n\
         # a heading with spaces\n\
         ||zzz.example^\n\
         #\n\
         # b\n\
         !\n\
         ||aaa.example^\n",
    )
    .unwrap();
    crate::fop_sort::fop_sort(&file, &config).unwrap();
    let result = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = result.lines().collect();
    // Text intact, and still above the rule it introduces rather than sorted
    // among the rules.
    assert!(
        lines.contains(&"# a heading with spaces"),
        "hash comment was rewritten: {result}"
    );
    // A lone `#` goes: one character, like any other too-short line. `# b`
    // reaches three and stays, as does a lone `!`.
    assert!(!lines.contains(&"#"), "a lone hash outlived the length minimum: {result}");
    assert!(lines.contains(&"# b"), "`# b` is long enough to stay: {result}");
    assert!(lines.contains(&"!"), "a lone `!` must keep its pass: {result}");
    let heading = lines.iter().position(|l| *l == "# a heading with spaces").unwrap();
    let zzz = lines.iter().position(|l| *l == "||zzz.example^").unwrap();
    let b_head = lines.iter().position(|l| *l == "# b").unwrap();
    assert!(heading < zzz && zzz < b_head, "comments did not hold the rules apart: {result}");
    // A comment closes the section, so the two rules never sort together.
    assert!(
        lines.iter().position(|l| *l == "||aaa.example^").unwrap() > b_head,
        "sections merged across the comment: {result}"
    );
    // Without the whitespace it stays a rule, as before.
    assert_eq!(crate::fop_sort::tidy_rule("#foo bar", &config), "#foobar");
    assert_eq!(crate::fop_sort::tidy_rule("# foo bar", &config), "# foo bar");
    // The addition checks must agree with the sort, or --remove-bad-rules
    // deletes a line the sort keeps: a heading ending in `##` read as
    // "separator with no selector", which is removable.
    for heading in ["# ends with ##", "# note: ||x.com^$doc", "# a heading", "#"] {
        assert!(
            crate::fop_rules::check_rule(heading).is_none(),
            "{heading} was judged as a rule"
        );
    }
    // A domainless cosmetic rule is not a comment and is still judged.
    assert!(crate::fop_rules::check_rule("##").is_some(), "real defect went unflagged");
    assert_eq!(crate::fop_sort::tidy_rule("##.ad", &config), "##.ad");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_regex_pseudo_arguments_kept() {
    // Their arguments are regexes, where `+` and `>` are not combinators:
    // tidied as a selector, `/__adv+/` became `/__adv + /`.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    for rule in [
        "a.com##div:matches-property(/__adv+/)",
        "a.com#?#div:matches-property(\"__vue__.adv\")",
        "a.com#?#div:matches-css-before(content: /a+b/)",
        "a.com##div:matches-css-after(content:/^ad>x/)",
    ] {
        assert_eq!(crate::fop_sort::tidy_rule(rule, &config), rule);
    }
}

#[test]
fn test_combine_filters_through_the_sort() {
    // The same, end to end through fop_sort, which sorts rules together
    // before merging. A rewrite may move logic between the two, so both are
    // pinned.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-combine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sort = |name: &str, lines: &[&str]| -> Vec<String> {
        let file = dir.join(name);
        std::fs::write(&file, format!("! Title: pin\n{}\n", lines.join("\n"))).unwrap();
        crate::fop_sort::fop_sort(&file, &config).unwrap();
        std::fs::read_to_string(&file).unwrap().lines().skip(1).map(String::from).collect()
    };
    assert_eq!(
        sort("cosmetic.txt", &[
            "c.com##.ad", "a.com##.banner", "b.com##.ad", "~x.com##.ad", "##.ad",
            "z.com#@#.ad", "y.com#@#.ad", "x.com##div:has-text(A)", "x.com##div:has-text(B)",
        ]),
        pinned(&["b.com,c.com##.ad", "~x.com##.ad", "##.ad", "y.com,z.com#@#.ad", "a.com##.banner", "x.com##div:has-text(/A|B/)"])
    );
    assert_eq!(
        sort("network.txt", &["||t.com^$script,domain=b.com", "||t.com^$script,domain=a.com", "||t.com^$image,domain=c.com"]),
        pinned(&["||t.com^$image,domain=c.com", "||t.com^$script,domain=a.com|b.com"])
    );
    // A section takes one mode from its make-up (fop_sort's
    // `element_lines > filter_lines`), and only that mode's rules merge.
    // Mostly cosmetic: the network rules are carried along, never merged.
    assert_eq!(
        sort("mixed.txt", &["c.com##.ad", "b.com##.ad", "a.com##.x", "||t.com^$script,domain=b.com", "||t.com^$script,domain=a.com"]),
        pinned(&["b.com,c.com##.ad", "a.com##.x", "||t.com^$script,domain=a.com", "||t.com^$script,domain=b.com"])
    );
    // A tie is not `>`, so it goes the other way: network mode, where the
    // network rules merge and the cosmetic ones do not.
    assert_eq!(
        sort("tie.txt", &["c.com##.ad", "b.com##.ad", "||t.com^$script,domain=b.com", "||t.com^$script,domain=a.com"]),
        pinned(&["b.com##.ad", "c.com##.ad", "||t.com^$script,domain=a.com|b.com"])
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_extension_value_keeps_its_spaces() {
    // AdGuard's `$extension=` names a userscript exactly. Stripped of spaces,
    // the name matched nothing and the exception stopped applying -- seven
    // rules in AdGuard's allowlist, silently.
    for rule in [
        "@@||usanetwork.com^$extension='AdGuard Assistant'",
        "@@||euroauto.ru^$extension='AdGuard Popup Blocker'|'AdGuard Popup Blocker (Beta)'",
    ] {
        assert_eq!(filter_tidy(rule, true), rule, "value altered: {}", rule);
    }
    // A pattern that merely contains the text is not an extension option, so
    // its spaces are still tidied as before.
    assert_eq!(filter_tidy("||x.com/extension=a b^", true), "||x.com/extension=ab^");
}

#[test]
fn test_regex_pattern_keeps_its_spaces_with_options() {
    // A space in a regex is part of what it matches: `[^&=? ]` excludes spaces
    // and `[^&=?]` does not. Only an option-less regex was recognised, so this
    // AdGuard rule had its character class rewritten, broadening the match.
    let rule = "/^https:\\/\\/[a-z]+\\.com\\/en\\/[a-z]+\\?([a-z]+=[^&=? ]*&)*id=[12][0-9]/$script,third-party,match-case";
    let tidied = filter_tidy(rule, true);
    assert!(tidied.contains("[^&=? ]"), "regex space stripped: {}", tidied);
    // Exceptions too.
    let exc = "@@/^https?:\\/\\/[^ ]+\\/ads\\//$script";
    assert!(filter_tidy(exc, true).contains("[^ ]+"), "{}", filter_tidy(exc, true));
    // An option-less regex was already kept.
    assert_eq!(filter_tidy("/a b/", true), "/a b/");
    // A plain network rule's stray space is still removed.
    assert_eq!(filter_tidy("||x.com/a b^$script", true), "||x.com/ab^$script");
}

#[test]
fn test_typo_fix_skips_cosmetic_rules_with_regex_domains() {
    // A cosmetic domain list may mix plain and regex domains; a regex ends in
    // `$/`, which read as an option marker. The `$option.option` fix then
    // turned `Math.random` into `Math,random`, splitting the scriptlet's
    // argument. From uAssets' quick-fixes.
    for rule in [
        "0deh.com,/^filemoon-[a-z0-9]+\\.(?:com|xyz)$/##+js(acs, Math.random, parseInt(localStorage)",
        "a.com,/^b-[a-z]+\\.com$/##+js(set, a.b.c, true)",
        "a.com,/^b\\.com$/#@#.ad.banner",
    ] {
        assert_eq!(filter_tidy(rule, true), rule, "cosmetic rule rewritten: {}", rule);
    }
    // The fix still applies to network rules.
    assert_eq!(filter_tidy("||a.com^$third-party.script", true), "||a.com^$script,third-party");
    assert_eq!(filter_tidy("@@||a.com^$image.script", true), "@@||a.com^$image,script");
}


#[test]
fn test_a_hosts_file_survives_without_the_localhost_flag() {
    // `filter_tidy` strips whitespace from anything that is not an element
    // rule, so every entry in a hosts file sorted as a filter list came out as
    // `0.0.0.0host` -- silently, since nothing downstream reads a mangled
    // entry as an error. listefr carries hosts.txt beside liste_fr.txt, so one
    // `fop .` over that repo rewrote all 6079 of its entries.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-hosts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("hosts.txt");
    std::fs::write(
        &file,
        concat!(
            "################################\n",
            "# Title : Test hosts\n",
            "################################\n",
            "0.0.0.0 zulu.example.com\n",
            "0.0.0.0 alpha.example.com\n",
            "127.0.0.1 mike.example.org\n",
        ),
    )
    .unwrap();
    crate::fop_sort::fop_sort(&file, &config).unwrap();
    let sorted = std::fs::read_to_string(&file).unwrap();
    let rules: Vec<&str> =
        sorted.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
    // Kept as written -- the space between address and host is the syntax.
    assert_eq!(
        rules,
        vec![
            "0.0.0.0 alpha.example.com",
            "127.0.0.1 mike.example.org",
            "0.0.0.0 zulu.example.com",
        ],
        "hosts entries not kept and ordered by host: {:?}",
        rules
    );
    // The `#` banner stays a comment, so it is not read as `##` plus an id
    // selector and the entries below it stay one section.
    assert!(sorted.starts_with("################################\n# Title : Test hosts\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_a_filter_list_is_not_taken_for_a_hosts_file() {
    // Detection licenses reading `#` as a comment, so a filter list must never
    // trip it: in a file taken for a hosts file every `##.ad` rule is read as
    // a comment and written back untouched. That is invisible if you only look
    // for the line -- a comment is written verbatim -- so these rules are ones
    // the tidier changes, and the test asserts they were changed.
    let chars = vec!["!".to_string()];
    let config = test_sort_config(&chars);
    let dir = std::env::temp_dir().join(format!("fop-test-nothosts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // A list whose rules are all generic hide rules, with entries mixed in.
    // Every rule here opens with `#`, so skipping `#` on sight would leave the
    // entries as the only lines counted and carry the whole file.
    let file = dir.join("a.txt");
    let mut content = String::new();
    for i in 0..60 {
        content.push_str(&format!("0.0.0.0 d{}.example.com\n", i));
    }
    content.push_str("##div  >  p\n");
    content.push_str("##.ad  >  .banner\n");
    std::fs::write(&file, &content).unwrap();
    crate::fop_sort::fop_sort(&file, &config).unwrap();
    let sorted = std::fs::read_to_string(&file).unwrap();
    for (raw, tidied) in [("##div  >  p", "##div > p"), ("##.ad  >  .banner", "##.ad > .banner")] {
        assert!(
            sorted.lines().any(|l| l == tidied),
            "{:?} was not tidied to {:?}: the file was taken for a hosts file\n{}",
            raw, tidied, sorted
        );
    }
    // And the entries are still kept as written: the line-level guard does not
    // depend on the file being recognised.
    assert!(
        sorted.lines().any(|l| l == "0.0.0.0 d0.example.com"),
        "hosts entry mangled in a file that is not a hosts file"
    );

    // The same list with the entries at the top, which is what a head sample
    // would see: still a filter list.
    let file2 = dir.join("b.txt");
    let mut content2 = String::new();
    for i in 0..60 {
        content2.push_str(&format!("0.0.0.0 d{}.example.com\n", i));
    }
    content2.push_str("example.com##div  >  p\n");
    std::fs::write(&file2, &content2).unwrap();
    crate::fop_sort::fop_sort(&file2, &config).unwrap();
    let sorted2 = std::fs::read_to_string(&file2).unwrap();
    assert!(
        sorted2.lines().any(|l| l == "example.com##div > p"),
        "a rule past the head was not tidied: the head was sampled\n{}",
        sorted2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_explicit_localhost_keeps_entries_and_drops_only_the_rest() {
    // `--localhost` does two things: keep the entries as written, and drop
    // what is not an entry. Splitting the first out so it applies in every
    // mode left the second reading `if config.localhost { remove }` -- which
    // removed the entries too, emptying the file the flag exists to sort. No
    // test covered the flag end to end, so the whole suite passed.
    let chars = vec!["!".to_string()];
    let config = crate::fop_sort::SortConfig { localhost: true, ..test_sort_config(&chars) };
    let dir = std::env::temp_dir().join(format!("fop-test-lhflag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("hosts.txt");
    std::fs::write(
        &file,
        concat!(
            "# a heading\n",
            "0.0.0.0 zulu.example.com\n",
            "||not-an-entry.example.com^$script\n",
            "0.0.0.0 alpha.example.com\n",
        ),
    )
    .unwrap();
    crate::fop_sort::fop_sort(&file, &config).unwrap();
    let sorted = std::fs::read_to_string(&file).unwrap();
    let kept: Vec<&str> =
        sorted.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
    assert_eq!(
        kept,
        vec!["0.0.0.0 alpha.example.com", "0.0.0.0 zulu.example.com"],
        "--localhost did not keep exactly the entries: {:?}",
        kept
    );
}
