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
    pub(crate) fn new(reason: &'static str, detail: &'a str) -> Self {
        Self { reason, detail, suggestion: None, removable: true }
    }
}

/// Split a rule at its cosmetic separator, if it has one.
///
/// A `#` inside a network rule's path is not a separator, so the separator
/// must match one of the known spellings exactly rather than any `#`.
///
/// The earliest separator wins, whichever family it belongs to. Trying `#`
/// first split `example.com$$div[attr="a##b"]` at the `##` inside the quotes,
/// leaving a selector of `b"]` that read as unbalanced.
#[inline]
fn split_cosmetic(line: &str) -> Option<(&str, &str, &str)> {
    match (split_hash_separator(line), split_html_filter(line)) {
        // The domain part's length is the separator's position.
        (Some(hash), Some(html)) => Some(if hash.0.len() <= html.0.len() { hash } else { html }),
        (hash, html) => hash.or(html),
    }
}

/// Split at the first `#`-family separator (`##`, `#@#`, `#?#`, `#$#` ...).
#[inline]
fn split_hash_separator(line: &str) -> Option<(&str, &str, &str)> {
    let mut from = 0;
    while let Some(hash) = line[from..].find('#') {
        let at = from + hash;
        // Shares the sorter's matcher rather than keeping a second list of the
        // same ten separators: one of them is enough to keep in step.
        if let Some((sep, _)) = crate::fop_sort::cosmetic_separator(&line[at..]) {
            return Some((&line[..at], sep, &line[at + sep.len()..]));
        }
        from = at + 1;
    }
    None
}

/// Split an AdGuard HTML-filtering rule, `domains$$selector` or its exception
/// `domains$@$selector`.
///
/// These carry no `#`, so the search in `split_cosmetic` never found them and
/// the rule fell through to the network path. There a bare tag -- `$$amp-consent`,
/// `$$advertisement-module`, both live in AdGuard Annoyances -- parsed as an
/// option list and was reported as an unknown option: a defect, so
/// `--remove-bad-rules` deleted the rule and `--ci` failed on it. Most `$$` rules
/// escaped only because `script[tag-content=...]` does not parse as options.
///
/// The domain part is held to the shape `ADGUARD_ELEMENT_PATTERN` allows, so a
/// network rule whose path happens to hold `$$` (`||a.com/$$p^$script`) is not
/// mistaken for one: a `/`, `|`, `@`, `"` or `!` before the separator rules it out.
#[inline]
fn split_html_filter(line: &str) -> Option<(&str, &str, &str)> {
    let at = match (line.find("$$"), line.find("$@$")) {
        (Some(a), Some(b)) => a.min(b),
        (a, b) => a.or(b)?,
    };
    let domains = &line[..at];
    if domains.bytes().any(|b| matches!(b, b'/' | b'|' | b'@' | b'"' | b'!')) {
        return None;
    }
    let sep = if line[at..].starts_with("$@$") { "$@$" } else { "$$" };
    Some((domains, sep, &line[at + sep.len()..]))
}

/// Constructs whose arguments are literal text, not CSS.
///
/// `+js(nostif, '0x)` and `:has-text(}(window);)` carry quotes, braces and
/// parens that are ordinary characters -- balancing them flags valid rules, so
/// a selector containing any of these is left unbalanced-checked.
///
/// The text-matching subset of `fop_sort::EXTENDED_PSEUDO`, and it must stay
/// that: `:-abp-contains(` was missing, so `div:-abp-contains(Don't miss)` read
/// its apostrophe as an unclosed quote and was deleted as "unbalanced". A test
/// now fails if a text-matching entry is added there and not here.
pub(crate) const LITERAL_ARG_CONSTRUCTS: [&str; 8] = [
    "+js(", ":has-text(", ":contains(", ":-abp-contains(", ":-abp-properties(",
    ":matches-", ":xpath(", ":watch-attr(",
];

/// Whether the selector's brackets, parens and braces balance.
///
/// Quote- and escape-aware, because `[href="("]` and `:has-text(/\)/)` both
/// carry deliberately unbalanced characters inside a string or a regex.
#[inline]
pub(crate) fn brackets_balance(selector: &str) -> bool {
    brackets_balance_with(selector, true)
}

/// `brackets_balance`, with backslash escaping optional.
///
/// AdGuard's HTML-filtering selectors (`$$script[tag-content="..."]`) escape a
/// quote by doubling it, and a backslash there is ordinary text: reading one as
/// an escape made `[tag-content="C:\"]` swallow its closing quote, so a valid
/// rule was reported unbalanced and deleted. A doubled quote needs no special
/// case -- it closes and reopens the string, which balances either way.
#[inline]
fn brackets_balance_with(selector: &str, backslash_escapes: bool) -> bool {
    let (mut square, mut round, mut curly) = (0i32, 0i32, 0i32);
    let mut quote = 0u8;
    let mut escaped = false;
    for b in selector.bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        match b {
            b'\\' if backslash_escapes => escaped = true,
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

/// A real hostname that lost its `||`.
const NO_ANCHOR_HOST: &str = "host rule with no || anchor -- matches the name anywhere";
/// A dotless token that is not a domain at all.
const NO_ANCHOR_MASH: &str = "unanchored pattern with no domain -- matches this text anywhere";

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
    if line.starts_with(['|', '@', '/', '.', '*']) {
        return None;
    }
    // `-` `+` `_` are boundary characters rather than syntax, so mash can wear
    // one as a disguise. They are stripped for the mash test only: `-ad.com^`
    // keeps its boundary deliberately, and advising `||ad.com^` for it would
    // throw that away.
    let mash = |text: &str| is_bare_token(text.trim_start_matches(['-', '+', '_']));
    let Some((host, rest)) = line.split_once('^') else {
        // No `^` at all. A hostname here is the bare-domain case, which has
        // its own check and better advice, so only the mash is ours.
        return mash(line).then_some(NO_ANCHOR_MASH);
    };
    // `example.com^somepath` is not a host rule and does not match the name
    // anywhere, so the advice would misdescribe it.
    if !rest.is_empty() && rest != "|" {
        return None;
    }
    if looks_like_hostname(host) {
        // A real hostname, so the anchored form is the obvious intent.
        Some(NO_ANCHOR_HOST)
    } else if mash(host) {
        // No domain at all: naming what it is beats guessing what was meant.
        Some(NO_ANCHOR_MASH)
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

/// Reason text for a space in a pattern.
///
/// Not "cannot match": ABP normalises spaces out of network filters, so such a
/// rule does work there. uBO and AdGuard do not, and fop's own `filter_tidy`
/// removes them, which is why this is advice with a repair rather than a
/// defect -- sorting the file fixes it losslessly.
const SPACE_REASON: &str = "space in the pattern -- uBO and AdGuard will not match this";

/// Whether `line` carries the syntax of a standard adblock network rule.
///
/// The anchors, or a separator, or an option list -- the marks that say the
/// author was writing a rule rather than a bare word or a hosts entry.
#[inline]
fn is_standard_network_rule(line: &str) -> bool {
    // A regex filter keeps its spaces -- `@@/^https?:\/\/[^ ]+\/ads\//` means
    // what it says -- so it is never judged on them.
    let body = line.trim_start_matches(['@', '|']);
    if body.starts_with('/') {
        return false;
    }
    // An anchor has to be followed by something rule-shaped. `@@ -3,6 +3,9 @@`
    // in a patch and `| Option | Description |` in a table both open with one
    // and are not rules; a real anchor is never followed by whitespace.
    let anchored = (line.starts_with('|') || line.starts_with("@@"))
        && body.starts_with(|c: char| !c.is_whitespace());
    anchored || line.ends_with('^')
}

/// Split a network rule into its pattern and option list.
///
/// Does by hand what `OPTION_PATTERN` did: the regex leads with `.*`, and on a
/// rule carrying options it cost some 650ns against 15ns for the byte paths --
/// forty times the rest of the checks put together. An option list is a
/// `$` followed by `~?[\w-]+` keys with optional `=value`, so recognising one
/// needs a scan, not a regex.
#[inline]
pub(crate) fn split_options(line: &str) -> Option<(&str, &str)> {
    split_options_inner(line, true)
}

/// The same split, tokenising the option list on every comma rather than only
/// unescaped ones.
///
/// `OPTION_PATTERN`, which this replaces in the sorter, reads a value as
/// `[^,\s]+`, so it stops at an escaped comma too: it reads
/// `$replace=/a\,b/c/` as `replace=/a\` and `b/c/`, decides `b/c/` is no
/// option key, and declines the whole line. 282 rules of 2.6M across four
/// corpora are shaped that way -- `$replace=` bodies, mostly -- and they take
/// the sorter's no-options path today, their option lists left as written.
/// Keeping that, rather than quietly beginning to sort them, is what makes
/// this swap free of any change in output; whether they *should* be sorted is
/// its own question.
#[inline]
pub(crate) fn split_options_as_pattern(line: &str) -> Option<(&str, &str)> {
    split_options_inner(line, false)
}

#[inline]
fn split_options_inner(line: &str, escaped_commas: bool) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    // Try each unescaped `$` from the right. The last one is usually the
    // marker, but a value may contain one -- `$removeparam=/^utm$/` ends in a
    // regex terminator -- and then the marker is further left. The regex this
    // replaces found it by backtracking; this walks the same candidates.
    for (at, _) in bytes
        .iter()
        .enumerate()
        .rev()
        .filter(|(i, &b)| b == b'$' && (*i == 0 || bytes[i - 1] != b'\\'))
    {
        let (pattern, options) = (&line[..at], &line[at + 1..]);
        if options.is_empty() {
            continue;
        }
        // Every option must be shaped like one, or this `$` was not the
        // marker: a shell `$PATH:/usr/bin` has a `:` no option key may carry.
        // Iterated rather than collected: the plain split needs no Vec, and
        // this runs for every `$` in every rule the sorter tidies.
        let is_shaped = |option: &str| {
            let option = option.strip_prefix('~').unwrap_or(option);
            let (key, value) = match option.split_once('=') {
                Some((k, v)) => (k, Some(v)),
                None => (option, None),
            };
            !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                // An empty value is not an option list to the pattern this
                // replaces, which leaves `$domain=` to the anchored fallback.
                && value.is_none_or(|v| !v.is_empty() && !v.bytes().any(|b| b.is_ascii_whitespace()))
        };
        let shaped = if escaped_commas {
            crate::fop_sort::split_unescaped_commas(options).into_iter().all(is_shaped)
        } else {
            options.split(',').all(is_shaped)
        };
        if shaped {
            return Some((pattern, options));
        }
    }
    None
}

/// Reason text for a bare word offered as a rule.
pub const NON_DOMAIN_REASON: &str =
    "not a domain -- a bare word matches any URL containing it";

/// Whether a line is a bare word: no dot, no separator, no options, no anchor,
/// and no `-` or `_` at either end.
///
/// Such a rule is legal and sometimes meant: `fingerprintjs` and `pkaystream`
/// are real. It is also what a stray paste looks like -- `isCookiesAccepted`,
/// one argument of a scriptlet, sorted quietly into a list because nothing
/// objected. Nothing here can tell those apart, which is why the ordinary
/// checks leave both alone and only `--remove-non-domain-on-add` acts.
///
/// The edge marker is the one signal that does separate them in practice. Of
/// 112 distinct bare-word rules across easylist, uAssets, AdguardFilters and
/// test-lists, 103 open or close with `-` or `_` -- how a substring rule is
/// written -- and those are never flagged. The other 9 are, and all 9 are real
/// rules, so the flag is for a list whose author knows they do not write them.
///
/// Counted over filter lists only. A first pass took in easylist's
/// `cleaned-domains.txt`, which is the banned-domain registry rather than a
/// list, and uAssets' `badlists.txt`, which names other lists; those put the
/// figure at 911 and lent it two rules, `fingerprintjs` and `pkaystream`,
/// that are not rules at all.
pub fn is_non_domain_word(line: &str) -> bool {
    let bytes = line.as_bytes();
    let edge = |b: u8| b == b'-' || b == b'_';
    bytes.len() >= 2
        && !line.contains('.')
        && bytes.first().is_some_and(|&b| !edge(b))
        && bytes.last().is_some_and(|&b| !edge(b))
        && bytes.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
    // Nor a plain-text comment, which the sort keeps verbatim. Only that form:
    // `##.ad` is a real generic cosmetic rule and is still judged. A heading
    // ending in `##` otherwise read as "separator with no selector", which
    // `--remove-bad-rules` was free to delete.
    if crate::fop_sort::is_plain_comment(line) {
        return None;
    }
    if !line.bytes().any(|b| b == b'#' || b == b'$') {
        // No separator and no options: the only thing left worth saying is
        // that a bare hostname was probably meant to be an anchored rule.
        // No detail: the line itself is already printed beside the reason.
        // No options, so the whole line is the pattern.
        if line.bytes().any(|b| b.is_ascii_whitespace()) {
            return is_standard_network_rule(line).then(|| RuleProblem {
                removable: false,
                    ..RuleProblem::new(SPACE_REASON, "")
            });
        }
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
        // `#$#` is two unrelated things. AdGuard uses it to inject CSS
        // (`##.ad { display: none !important; }`), which is selector-shaped and
        // worth balancing; ABP uses it to invoke a snippet
        // (`#$#hide-if-contains 'x' p[id]`), whose arguments are regex literals
        // and quoted strings where a bracket is data, not syntax. The injection
        // opens on a CSS selector and carries a ` {` declaration block; a
        // snippet opens on its name and never does. Testing the opening rather
        // than a trailing `}` keeps a truncated injection (`.ad { color: red`)
        // catchable, and testing ` {` rather than any `{` keeps a snippet whose
        // argument holds one -- pluto.tv's regex quantifier `{1,2}` -- from
        // reading as CSS. Without this, four valid rules in ABP's
        // anti-circumvention list were flagged "unbalanced brackets" (`\(`
        // inside a regex, `[^>]` inside an XPath) and deleted.
        let is_snippet = matches!(sep, "#$#" | "#@$#")
            && !selector.starts_with(['.', '#', '[', '*', ':'])
            && !selector.contains(" {");
        if selector.is_empty() {
            return Some(RuleProblem::new("separator with no selector", sep));
        }
        if !is_script && !domains_ok(domains) {
            return Some(RuleProblem::new("malformed domain list", domains));
        }
        // Every literal-argument construct contains a `(`, and a selector with
        // no `(` cannot be unbalanced in one either -- so one byte search gates
        // both the six substring scans and the balance walk.
        let has_paren = !is_script && !is_snippet && selector.as_bytes().contains(&b'(');
        let literal_args =
            has_paren && LITERAL_ARG_CONSTRUCTS.iter().any(|c| selector.contains(c));
        // An HTML-filtering selector's backslashes are literal (see
        // `brackets_balance_with`); its regexes live in `:contains()`, which
        // `literal_args` already exempts.
        let html_filter = matches!(sep, "$$" | "$@$");
        if !is_script
            && !is_snippet
            && !literal_args
            && !brackets_balance_with(selector, !html_filter)
        {
            return Some(RuleProblem::new("unbalanced brackets in selector", selector));
        }
        // A selector cannot open on a combinator. `+js(...)` is a scriptlet
        // injection, not a sibling combinator, so it is exempt.
        let first = selector.as_bytes()[0];
        if !is_script && !is_snippet && (first == b'>' || (first == b'+' && !selector.starts_with("+js("))) {
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
    let Some((pattern, options)) = split_options(line) else {
        // The pattern rejects a malformed option list and a line that is not a
        // rule alike. Telling them apart is only safe behind an unambiguous
        // filter-rule anchor, where a `$` cannot be a shell variable or a
        // regex terminator in someone's source.
        let anchored =
            line.starts_with("||") || line.starts_with('|') || line.starts_with("@@");
        // No space check here. Reaching this branch means the option list did
        // not parse, and one reason it may not is a value that legitimately
        // holds spaces -- uBO's `$replace=` rewrites carry whole snippets of
        // HTML. Without a parse there is no way to say where the pattern ends,
        // and guessing flagged three valid rules in uAssets as defects.
        // `OPTION_PATTERN` rejects any option whose value holds a space, such
        // as `$csp=script-src 'none'`. The pattern half is still worth judging
        // for mash -- see the note at the end of this function for why only
        // that half.
        if let Some((pattern, _)) = line.rsplit_once('$') {
            if let Some(reason) = unanchored_mash(pattern) {
                return Some(RuleProblem { removable: false, ..RuleProblem::new(reason, "") });
            }
        }
        // A rule with no `$` at all has no option list to be malformed.
        if let (true, Some((_, tail))) = (anchored, line.rsplit_once('$')) {
            for option in crate::fop_sort::split_unescaped_commas(tail) {
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
    // The pattern half of a network rule never contains whitespace: across
    // 609k lines of EasyList and the region lists, not one does. In a rule it
    // is a defect; anywhere else -- `some: $value` in a YAML file -- it just
    // means this was never a rule.
    if pattern.bytes().any(|b| b.is_ascii_whitespace()) {
        return is_standard_network_rule(pattern).then(|| RuleProblem {
            removable: false,
            ..RuleProblem::new(SPACE_REASON, "")
        });
    }
    // Some modifiers are valid bare only on an exception, where they switch off
    // every rule of that kind for the site (`@@||site^$removeheader`).
    let exception = line.starts_with("@@");
    // Commas inside a `jsonprune=`/`xmlprune=` value are part of the value.
    for option in crate::fop_sort::split_filter_options(options) {
        let option = option.trim();
        let stripped = option.trim_start_matches('~');
        if let Some((key, value)) = stripped.split_once('=') {
            if value.is_empty() {
                return Some(RuleProblem::new("option with no value", option));
            }
            if !crate::is_known_option_in(stripped, exception)
                && !crate::is_known_option_in(key, exception)
            {
                return Some(RuleProblem {
                    suggestion: crate::suggest_option(key),
                    ..RuleProblem::new("unknown option", option)
                });
            }
            if PIPE_VALUED.contains(&key) && !pipe_values_ok(value) {
                return Some(RuleProblem::new("empty entry in option value", option));
            }
        } else if !crate::is_known_option_in(stripped, exception) {
            return Some(RuleProblem {
                suggestion: crate::suggest_option(stripped),
                ..RuleProblem::new("unknown option", option)
            });
        }
    }
    // The pattern may still be mash, but a *hostname* here is left alone. The
    // host check used to run on this path too, on the grounds that an
    // unanchored host should not escape by its options -- but the opposite is
    // true: `$csp=` and `$redirect-rule=` are not written by accident, and the
    // author who wrote one chose the matching as well. ABP's own
    // anti-circumvention list publishes 13 such rules
    // (`billboard.com^$csp=script-src-attr \'none\'` and friends) and uAssets
    // another in `host-cdn.net^$image,redirect-rule=32x32.png,...`; all were
    // flagged, and `--remove-bad-rules` then deleted advice along with
    // defects, so all were deleted. It now deletes defects only. The bare forms this was meant to catch --
    // `example.com^`, `exa mple.com^` -- carry no options and are still caught
    // where the whole line is the pattern. Mash stays flagged either way: a
    // dotless, vowel-less token is not a deliberate choice in any list.
    if let Some(reason) = unanchored_mash(pattern) {
        return Some(RuleProblem { removable: false, ..RuleProblem::new(reason, "") });
    }
    None
}

/// `unanchored_reason` restricted to the mash case, for rules carrying options.
#[inline]
fn unanchored_mash(pattern: &str) -> Option<&'static str> {
    unanchored_reason(pattern).filter(|&r| r == NO_ANCHOR_MASH)
}

/// The part of a rule that a merge leaves unchanged.
///
/// The sort combines rules that differ only in their domain list, or only in
/// the argument of a text-matching pseudo-class, so a line flagged in a file
/// that was already sorted may carry a committed rule merged into it. Two rules
/// sharing this key may be halves of one such merge. Deliberately loose -- the
/// whole pattern of a network rule, the separator and selector of a cosmetic
/// one -- because it is only ever used to decide against deleting.
///
/// The text-matching case is keyed through `fop_sort::parse_has_text_selector`,
/// the parser the merge itself groups by, rather than a list of pseudo-class
/// names kept here: a hand-kept `:has-text(` missed `:-abp-contains(` and
/// `:abp-contains(`, which the sort merges too, so a merged line got a key its
/// committed half did not share and was deleted with it.
pub(crate) fn merge_key(line: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let line = line.trim();
    if let Some((domains, sep, selector)) = split_cosmetic(line) {
        return match crate::fop_sort::parse_has_text_selector(selector) {
            Some((base, pseudo, _)) => Cow::Owned(format!("{}{}:{}", sep, base, pseudo)),
            None => Cow::Borrowed(&line[domains.len()..]),
        };
    }
    Cow::Borrowed(match split_options(line) {
        Some((pattern, _)) => pattern,
        None => line,
    })
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
