//! Validity checks for newly added filter rules.
//!
//! Deliberately separate from `malformed_rule_reason`, which runs over every
//! line of every file and so must only ever match rules that are impossible.
//! These checks run on lines the author just added, with the author present:
//! a false positive costs one glance, so they can be stricter.

use crate::fop_typos::Addition;


/// What is wrong with a rule, and the fragment that proves it.
pub struct RuleProblem<'a> {
    pub reason: &'static str,
    /// The offending fragment, empty when the whole line is the evidence.
    pub detail: &'a str,
    /// What the fragment was probably meant to be.
    pub suggestion: Option<&'static str>,
    /// Whether --remove-bad-rules may delete this line.
    ///
    /// False for advice rather than a defect: a bare domain is legal syntax,
    /// and in a plain domain-list file it is exactly what belongs there, so
    /// deleting one would destroy a deliberate entry.
    pub removable: bool,
}

impl<'a> RuleProblem<'a> {
    #[inline]
    fn new(reason: &'static str, detail: &'a str) -> Self {
        Self { reason, detail, suggestion: None, removable: true }
    }
}

/// The cosmetic separators, longest first so `#@?#` wins over `#@#`.
const SEPARATORS: [&str; 10] = [
    "#@$?#", "#@%#", "#@$#", "#@?#", "#$?#", "#@#", "#$#", "#%#", "#?#", "##",
];

/// Split a rule at its cosmetic separator, if it has one.
///
/// A `#` inside a network rule's path is not a separator, so the separator
/// must match one of the known spellings exactly rather than any `#`.
#[inline]
fn split_cosmetic(line: &str) -> Option<(&str, &str, &str)> {
    let mut from = 0;
    while let Some(hash) = line[from..].find('#') {
        let at = from + hash;
        for sep in SEPARATORS {
            if line[at..].starts_with(sep) {
                return Some((&line[..at], sep, &line[at + sep.len()..]));
            }
        }
        from = at + 1;
    }
    None
}

/// Constructs whose arguments are literal text, not CSS.
///
/// `+js(nostif, '0x)` and `:has-text(}(window);)` carry quotes, braces and
/// parens that are ordinary characters -- balancing them flags valid rules, so
/// a selector containing any of these is left unbalanced-checked.
const LITERAL_ARG_CONSTRUCTS: [&str; 6] = [
    "+js(", ":has-text(", ":contains(", ":matches-", ":xpath(", ":watch-attr(",
];

/// Whether the selector's brackets, parens and braces balance.
///
/// Quote- and escape-aware, because `[href="("]` and `:has-text(/\)/)` both
/// carry deliberately unbalanced characters inside a string or a regex.
#[inline]
pub(crate) fn brackets_balance(selector: &str) -> bool {
    let (mut square, mut round, mut curly) = (0i32, 0i32, 0i32);
    let mut quote = 0u8;
    let mut escaped = false;
    for b in selector.bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        match b {
            b'\\' => escaped = true,
            b'"' | b'\'' if quote == 0 => quote = b,
            q if q == quote => quote = 0,
            _ if quote != 0 => {}
            b'[' => square += 1,
            b']' => square -= 1,
            b'(' => round += 1,
            b')' => round -= 1,
            // AdGuard CSS injection (`#$#.ad { display: none; }`) and scriptlet
            // bodies both carry braces, so they balance like the rest.
            b'{' => curly += 1,
            b'}' => curly -= 1,
            _ => {}
        }
        // A close before its open is already unbalanced; stop early.
        if square < 0 || round < 0 || curly < 0 {
            return false;
        }
    }
    square == 0 && round == 0 && curly == 0 && quote == 0
}


/// Whether a `,`-separated domain list is well formed.
///
/// Only the unarguable faults: an empty entry, or a doubled dot. A regex
/// domain (`/^x\d+$/##.ad`) is left alone -- `..` is ordinary inside one.
#[inline]
fn domains_ok(domains: &str) -> bool {
    if domains.is_empty() || domains.starts_with('/') {
        return true;
    }
    domains.split(',').all(|d| {
        let d = d.trim().trim_start_matches('~');
        !d.is_empty() && !d.contains("..")
    })
}

/// Whether a `|`-separated option value is well formed.
///
/// A regex value (`domain=/re|gex/`) keeps its pipes, so it is skipped.
#[inline]
fn pipe_values_ok(value: &str) -> bool {
    if value.starts_with('/') {
        return true;
    }
    value.split('|').all(|v| !v.trim().trim_start_matches('~').is_empty())
}

/// Options whose value is a `|`-separated list rather than free text.
const PIPE_VALUED: [&str; 4] = ["domain", "denyallow", "from", "to"];

/// Final labels that mark a substring pattern for a file, not a domain.
///
/// `_chartbeat.js` and `.cookielaw.js` are ordinary substring rules; without
/// this they read as `label.label` and look like hostnames.
const FILE_SUFFIXES: [&str; 24] = [
    "js", "css", "gif", "png", "jpg", "jpeg", "svg", "webp", "ico", "php",
    "html", "htm", "asp", "aspx", "jsp", "cgi", "json", "xml", "swf", "woff",
    "woff2", "mp4", "txt", "wasm",
];

/// Whether `line` is a bare hostname with no filter syntax around it.
///
/// Such a rule is legal -- it matches the text anywhere in a URL -- but it is
/// almost always meant to be `||host^`, and as written it also matches
/// `notdomain.com.evil.test` and any URL merely mentioning the name. Genuine
/// filter lists effectively never carry one: across 608k lines of EasyList and
/// the region lists, every instance was in a plain domain-list file.
#[inline]
fn is_bare_domain(line: &str) -> bool {
    // Any filter syntax at all means the author knew what they were writing.
    if line
        .bytes()
        .any(|b| matches!(b, b'|' | b'/' | b'^' | b'*' | b'=' | b':' | b' ' | b'\t' | b'?' | b'&' | b'@' | b'~' | b','))
    {
        return false;
    }
    looks_like_hostname(line)
}

/// Whether `text` is shaped like a hostname and nothing else.
#[inline]
fn looks_like_hostname(line: &str) -> bool {
    // Leading or trailing dots mark a substring pattern (`.cookielaw.js`).
    if line.starts_with('.') || line.ends_with('.') {
        return false;
    }
    let mut labels = line.split('.').peekable();
    let mut count = 0;
    let mut last = "";
    while let Some(label) = labels.next() {
        // A label is alphanumeric with inner hyphens; `_chartbeat` is not one.
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return false;
        }
        count += 1;
        if labels.peek().is_none() {
            last = label;
        }
    }
    // A TLD is alphabetic; a trailing `.js` or `.gif` is a filename.
    count >= 2
        && (2..=24).contains(&last.len())
        && last.bytes().all(|b| b.is_ascii_alphabetic())
        && !FILE_SUFFIXES.iter().any(|ext| ext.eq_ignore_ascii_case(last))
}

/// Whether `line` is a hostname rule that forgot its `||` anchor.
///
/// `rbush.shop^` is legal and matches that text anywhere in a URL, so it also
/// blocks `lampedburbush.shop` and anything else ending in the name -- the
/// over-blocking a missing anchor causes is invisible until someone reports a
/// broken site. Deliberate uses are vanishingly rare: across 609k lines of
/// EasyList and the region lists there was one, itself a typo.
#[inline]
fn unanchored_reason(line: &str) -> Option<&'static str> {
    // An anchor, a scheme, a wildcard or a leading dot all mean the author
    // chose the matching they wanted.
    if line.starts_with(['|', '@', '/', '.', '-', '*']) {
        return None;
    }
    let (host, rest) = line.split_once('^')?;
    // `example.com^somepath` is not a host rule and does not match the name
    // anywhere, so the advice would misdescribe it.
    if !rest.is_empty() && rest != "|" {
        return None;
    }
    if looks_like_hostname(host) {
        // A real hostname, so the anchored form is the obvious intent.
        Some("host rule with no || anchor -- matches the name anywhere")
    } else if is_bare_token(host) {
        // No domain at all: naming what it is beats guessing what was meant.
        Some("unanchored pattern with no domain -- matches this text anywhere")
    } else {
        None
    }
}

/// Whether `text` looks like keyboard mash rather than a word, as in
/// `fdfdgfgdgfd^`.
///
/// The form -- a dotless token terminated by `^` -- appears nowhere in 609k
/// lines of EasyList and the region lists, but that only says it is unused,
/// not that any such token is a mistake: `doubleclick^`, `prebid^` and
/// `300x250^` are all patterns someone could reasonably write, and flagging
/// them would see them deleted under `--remove-bad-rules`.
///
/// So the bar is a token that reads as nothing at all: six or more letters,
/// no digits or punctuation, and not one vowel. That catches the mash and
/// leaves every real keyword alone, at the cost of missing mash that happens
/// to contain a vowel.
#[inline]
fn is_bare_token(text: &str) -> bool {
    text.len() >= 6
        && text.bytes().all(|b| b.is_ascii_alphabetic())
        && !text.bytes().any(|b| matches!(b.to_ascii_lowercase(), b'a' | b'e' | b'i' | b'o' | b'u'))
}

/// Why this rule looks wrong, or `None` if it looks fine.
///
/// Ordered cheapest-first: a byte-level reject for comments and for lines
/// carrying neither `#` nor `$` means the great majority of additions leave
/// here without any scanning at all.
pub fn check_rule(line: &str) -> Option<RuleProblem<'_>> {
    let line = line.trim();
    // Comments, section headers, AdGuard rule modifiers (`[$path=...]`) and
    // hosts-style entries are not ours to judge.
    match line.as_bytes().first()? {
        b'!' | b'[' | b'%' => return None,
        _ => {}
    }
    if !line.bytes().any(|b| b == b'#' || b == b'$') {
        // No separator and no options: the only thing left worth saying is
        // that a bare hostname was probably meant to be an anchored rule.
        // No detail: the line itself is already printed beside the reason.
        if is_bare_domain(line) {
            return Some(RuleProblem {
                removable: false,
                ..RuleProblem::new("bare domain, did you mean ||host^ ?", "")
            });
        }
        return unanchored_reason(line).map(|reason| RuleProblem {
            removable: false,
            ..RuleProblem::new(reason, "")
        });
    }

    if let Some((domains, sep, selector)) = split_cosmetic(line) {
        // `#%#` injects JavaScript and `//scriptlet(...)` is a scriptlet call;
        // neither is a CSS selector, so brackets and combinators mean nothing
        // there and a lone apostrophe in a comment is not an unbalanced quote.
        let is_script = matches!(sep, "#%#" | "#@%#") || selector.starts_with("//");
        if selector.is_empty() {
            return Some(RuleProblem::new("separator with no selector", sep));
        }
        if !is_script && !domains_ok(domains) {
            return Some(RuleProblem::new("malformed domain list", domains));
        }
        // Every literal-argument construct contains a `(`, and a selector with
        // no `(` cannot be unbalanced in one either -- so one byte search gates
        // both the six substring scans and the balance walk.
        let has_paren = !is_script && selector.as_bytes().contains(&b'(');
        let literal_args =
            has_paren && LITERAL_ARG_CONSTRUCTS.iter().any(|c| selector.contains(c));
        if !is_script && !literal_args && !brackets_balance(selector) {
            return Some(RuleProblem::new("unbalanced brackets in selector", selector));
        }
        // A selector cannot open on a combinator. `+js(...)` is a scriptlet
        // injection, not a sibling combinator, so it is exempt.
        let first = selector.as_bytes()[0];
        if !is_script && (first == b'>' || (first == b'+' && !selector.starts_with("+js("))) {
            return Some(RuleProblem::new("selector starts with a combinator", selector));
        }
        return None;
    }

    // Network rule. The option list is recognised with the same pattern the
    // sorter uses, rather than by taking the last `$`: a pattern may legally
    // contain one (`$removeparam=/^utm$/`, `$replace=/(a)b/$1c/`), and a line
    // that is not a rule at all may contain one anywhere.
    let bytes = line.as_bytes();
    if bytes.last() == Some(&b'$') && bytes.len() > 1 && bytes[bytes.len() - 2] != b'\\' {
        return Some(RuleProblem::new("option marker with no options", ""));
    }
    let Some(caps) = crate::OPTION_PATTERN.captures(line) else {
        // The pattern rejects a malformed option list and a line that is not a
        // rule alike. Telling them apart is only safe behind an unambiguous
        // filter-rule anchor, where a `$` cannot be a shell variable or a
        // regex terminator in someone's source.
        let anchored =
            line.starts_with("||") || line.starts_with('|') || line.starts_with("@@");
        // `OPTION_PATTERN` rejects any option whose value holds a space, such
        // as `$csp=script-src 'none'`. The pattern half is still worth judging,
        // or an unanchored host escapes the check purely by its options.
        if let Some((pattern, _)) = line.rsplit_once('$') {
            let reason = unanchored_reason(pattern).or_else(|| {
                is_bare_domain(pattern)
                    .then_some("host rule with no || anchor -- matches the name anywhere")
            });
            if let Some(reason) = reason {
                return Some(RuleProblem { removable: false, ..RuleProblem::new(reason, "") });
            }
        }
        // A rule with no `$` at all has no option list to be malformed.
        if let (true, Some((_, tail))) = (anchored, line.rsplit_once('$')) {
            for option in tail.split(',') {
                if option.is_empty() {
                    return Some(RuleProblem::new("empty option", tail));
                }
                if option.ends_with('=') {
                    return Some(RuleProblem::new("option with no value", option));
                }
            }
        }
        return None;
    };
    // The pattern half of a network rule never contains whitespace, but
    // `OPTION_PATTERN` accepts anything before the `$`, so `some: $value` in a
    // YAML file would otherwise read as a rule with an unknown option.
    if caps.get(1)?.as_str().bytes().any(|b| b.is_ascii_whitespace()) {
        return None;
    }
    let options = caps.get(2)?.as_str();
    // Commas inside a `jsonprune=`/`xmlprune=` value are part of the value.
    for option in crate::fop_sort::split_filter_options(options) {
        let option = option.trim();
        let stripped = option.trim_start_matches('~');
        if let Some((key, value)) = stripped.split_once('=') {
            if value.is_empty() {
                return Some(RuleProblem::new("option with no value", option));
            }
            if !crate::is_known_option(stripped) && !crate::is_known_option(key) {
                return Some(RuleProblem {
                    suggestion: crate::suggest_option(key),
                    ..RuleProblem::new("unknown option", option)
                });
            }
            if PIPE_VALUED.contains(&key) && !pipe_values_ok(value) {
                return Some(RuleProblem::new("empty entry in option value", option));
            }
        } else if !crate::is_known_option(stripped) {
            return Some(RuleProblem {
                suggestion: crate::suggest_option(stripped),
                ..RuleProblem::new("unknown option", option)
            });
        }
    }
    // The options are sound; the pattern they hang off may still have lost its
    // anchor. Checked last so a real defect is reported ahead of this advice.
    let pattern = caps.get(1)?.as_str();
    let reason = unanchored_reason(pattern).or_else(|| {
        is_bare_domain(pattern).then_some("host rule with no || anchor -- matches the name anywhere")
    });
    if let Some(reason) = reason {
        return Some(RuleProblem { removable: false, ..RuleProblem::new(reason, "") });
    }
    None
}

/// Check added lines for bad rules.
pub fn check_additions(additions: &[Addition]) -> Vec<(&Addition, RuleProblem<'_>)> {
    additions
        .iter()
        .filter_map(|add| check_rule(&add.content).map(|problem| (add, problem)))
        .collect()
}

/// Report bad rules in additions (formatted output).
pub fn report_addition_problems(problems: &[(&Addition, RuleProblem)], no_color: bool) {
    if problems.is_empty() {
        return;
    }
    println!("\nQuestionable rules in added lines:");
    for (add, problem) in problems {
        let location = format!("{}:{}", add.file, add.line_num);
        // The fragment is what makes a report actionable -- "unknown option"
        // sends you hunting, "unknown option: thrid-party" does not.
        let mut why = if problem.detail.is_empty() {
            problem.reason.to_string()
        } else {
            format!("{}: {}", problem.reason, problem.detail)
        };
        if let Some(did_you_mean) = problem.suggestion {
            why.push_str(&format!(" -- did you mean {}?", did_you_mean));
        }
        if no_color {
            println!("  {}: {} ({})", location, add.content, why);
        } else {
            use owo_colors::OwoColorize;
            println!("  {}: {} ({})", location.cyan(), add.content, why.yellow());
        }
    }
}
