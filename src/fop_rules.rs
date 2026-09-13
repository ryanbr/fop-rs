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
}

impl<'a> RuleProblem<'a> {
    #[inline]
    fn new(reason: &'static str, detail: &'a str) -> Self {
        Self { reason, detail, suggestion: None }
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
fn brackets_balance(selector: &str) -> bool {
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
        return None;
    }

    if let Some((domains, sep, selector)) = split_cosmetic(line) {
        if selector.is_empty() {
            return Some(RuleProblem::new("separator with no selector", sep));
        }
        if !domains_ok(domains) {
            return Some(RuleProblem::new("malformed domain list", domains));
        }
        let literal_args = LITERAL_ARG_CONSTRUCTS.iter().any(|c| selector.contains(c));
        if !literal_args && !brackets_balance(selector) {
            return Some(RuleProblem::new("unbalanced brackets in selector", selector));
        }
        // A selector cannot open on a combinator. `+js(...)` is a scriptlet
        // injection, not a sibling combinator, so it is exempt.
        let first = selector.as_bytes()[0];
        if first == b'>' || (first == b'+' && !selector.starts_with("+js(")) {
            return Some(RuleProblem::new("selector starts with a combinator", selector));
        }
        return None;
    }

    // Network rule: everything after the last unescaped `$` is the option list.
    let dollar = line.bytes().enumerate().rev().find_map(|(i, b)| {
        (b == b'$' && (i == 0 || line.as_bytes()[i - 1] != b'\\')).then_some(i)
    })?;
    let options = &line[dollar + 1..];
    if options.is_empty() {
        return Some(RuleProblem::new("option marker with no options", ""));
    }
    for option in options.split(',') {
        let option = option.trim();
        if option.is_empty() {
            return Some(RuleProblem::new("empty option", options));
        }
        let stripped = option.trim_start_matches('~');
        if let Some((key, value)) = stripped.split_once('=') {
            if value.is_empty() {
                return Some(RuleProblem::new("option with no value", option));
            }
            // A value may itself contain commas (removeparam regexes, jsonprune
            // paths), so an unknown *key* is the signal, not an unknown option.
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
