# FOP Filter Parsing Reference

What FOP recognises, rewrites, merges and leaves alone, across Adblock Plus
(eyeo), uBlock Origin and AdGuard syntax.

The **ABP / uBO / AdGuard** columns say which blocker documents the syntax.
The **Lists** column says which snapshot in [`test-lists/`](test-lists) uses
it: **A** = AdGuard filters, **E** = eyeo's lists (the Eyeo folder), **U** =
uBlock filters (uAssets). The **FOP** column says what FOP does with it.
Snapshot usage was surveyed in September 2026.

- [How FOP reads a line](#how-fop-reads-a-line)
- [Network filter options](#network-filter-options)
- [Option conversion](#option-conversion)
- [Cosmetic rule separators](#cosmetic-rule-separators)
- [Pseudo-classes](#pseudo-classes)
- [Scriptlets, snippets and injection](#scriptlets-snippets-and-injection)
- [Domains](#domains)
- [Directives and hints](#directives-and-hints)
- [Whitespace](#whitespace)
- [Sorting and merging](#sorting-and-merging)
- [Rules removed with a warning](#rules-removed-with-a-warning)
- [Typo detection and fixing](#typo-detection-and-fixing)
- [Checks on added rules](#checks-on-added-rules)
- [Known limitations](#known-limitations)

## How FOP reads a line

| Line | Treated as | Notes |
|------|-----------|-------|
| `! ...` | Comment | Kept in place, and ends the section above it. Only the rules between two comments are sorted together. |
| `# ...` | Comment | The hosts-file convention, for lists that use it. Needs the whitespace and something after it: `#foo` is a rule, `# foo` a comment, and a lone `#` is neither, so the line-length minimum removes it. A lone `!` is exempt. Add other characters with `--comments=`. In a file recognised as a hosts file every `#` line is a comment, a lone `#` and a `####` banner included. |
| `[Adblock Plus 2.0]` | Header | Kept in place; ends a section |
| `%include file` | Include directive | Kept in place; ends a section |
| `!#if`, `!#else`, `!#endif`, `!#include`, `!#safari_cb_affinity` | Comment | Rules never cross a directive. See [Directives and hints](#directives-and-hints). |
| `!+ NOT_OPTIMIZED`, `!+ PLATFORM(...)` | AdGuard hint | The rule after it stays in place: tidied, never sorted or merged. See [Directives and hints](#directives-and-hints). |
| `domains##selector` and the other separators | Cosmetic rule | Domains lowercased and sorted, selector tidied, then merged. Which separators qualify depends on the mode; see [Cosmetic rule separators](#cosmetic-rule-separators). |
| Cosmetic rule the mode does not cover | Left as written | Sorted in place like a network rule, but its text is untouched. The option tidying never touches a line containing a cosmetic separator. |
| `/regex/##selector` (uBO regex domain) | Left as written | |
| `[$path=...]domain##selector` | Left as written | AdGuard's cosmetic modifiers |
| `/regex/` or `/regex/$options` | Regex network rule | The pattern is left as written, spaces included |
| `0.0.0.0 host`, `::1 host`, any IP then a host | Hosts entry | Always left exactly as written -- the space between address and host is the syntax, and tidying a rule would eat it. In a file recognised as a hosts file, also sorted by host. See [Hosts files](#hosts-files). |
| Anything else | Network rule | Options sorted and normalised |
| Empty line | Removed | Unless `--keep-empty-lines`, where it also ends a section |

Windows line endings (CRLF) are converted to LF, with a warning.

## Hosts files

A hosts file is an address, whitespace, then a hostname, and lists of them
ship beside filter lists -- listefr carries `hosts.txt` next to
`liste_fr.txt`. FOP recognises one and reads it on its own terms: `#` starts
a comment, and entries are ordered by host rather than by the whole line.

A file qualifies only if **every** rule in it is an entry. One cosmetic or
network rule anywhere disqualifies it, however many entries surround it.
Recognition is what licenses reading `#` as a comment, and in a file taken
for a hosts file a generic `##.ad` rule would be read as one -- so the test
is unanimity, not a majority, and not a sample of the first few lines.

`#` is the awkward character, since it opens a comment in a hosts file and
a rule in a filter list. While deciding, a `#` line counts as a comment
when it is `#` alone, a run of `#`, `#` before whitespace, `#` before
punctuation (`#=====`), or a word followed by a space (`#Title: my hosts`).
Anything that could begin a rule does not: `##.ad`, `#@#`, `#?#`, `#$#`,
`#%#`, and a bare word such as `#ad-banner`, which is a legal substring
rule. `##### AdAway #####` is genuinely ambiguous -- it parses as `##`
plus a selector -- and disqualifies a file.

A `!` comment and an `[Adblock Plus 2.0]` header are skipped rather than
held against a file, so a hosts file carrying either is still recognised.

Any address is an entry, not just the two a blocklist null-routes with: a
hosts file's own preamble is written in neither. The Debian and Ubuntu
default block that StevenBlack's lists keep at the top carries
`255.255.255.255 broadcasthost` and eight IPv6 lines -- `::1 ip6-localhost
ip6-loopback`, `fe00::0 ip6-localnet`, `ff02::3 ip6-allhosts`. Several
hostnames on one line, and tab separators, are kept as written.

An entry is never tidied, in any mode and whether or not its file was
recognised: `filter_tidy` strips whitespace from anything that is not a
cosmetic rule, which would turn `0.0.0.0 keep.com` into `0.0.0.0keep.com`.

Recognition decides formatting, never deletion. `--localhost` says the file
is a hosts file and drops any line that is not an entry; recognition is a
guess, so a stray rule in a recognised file is sorted as the rule it appears
to be. `--localhost-files=` forces the flag for named files.

## Network filter options

Recognised options pass the [checks on added rules](#checks-on-added-rules).
An option FOP does not know gives a warning while sorting. Under
`--check-rules-on-add` it is reported as a defect, and
`--remove-bad-rules` deletes the rule.

### Content types

| Option | ABP | uBO | AdGuard | Lists | FOP |
|--------|-----|-----|---------|-------|-----|
| `script` | Yes | Yes | Yes | A E U | |
| `image` | Yes | Yes | Yes | A E U | |
| `stylesheet` | Yes | Yes | Yes | A E U | `css` converts to it |
| `font` | Yes | Yes | Yes | A E | |
| `media` | Yes | Yes | Yes | A E U | |
| `object` | Yes | Yes | Yes | A E U | |
| `subdocument` | Yes | Yes | Yes | A E U | `frame` and `iframe` convert to it |
| `xmlhttprequest` | Yes | Yes | Yes | A E U | `xhr` and `xml` convert to it |
| `websocket` | Yes | Yes | Yes | A | |
| `ping` | Yes | Yes | Yes | E U | |
| `popup` | Yes | Yes | Yes | A E U | |
| `document` | Yes | Yes | Yes | A E U | `doc` converts to it |
| `other` | Yes | Yes | Yes | A E U | |
| `webrtc` | Yes | - | Removed | | Recognised |
| `object-subrequest` | Removed | - | Removed | | Recognised (legacy) |
| `webbundle` | - | - | - | | Recognised (a Web Bundle request type) |
| `popunder` | - | Yes | - | U | |
| `inline-script` | - | Yes | Yes | U | |
| `inline-font` | - | Yes | Yes | | |
| `beacon` | - | - | - | | Recognised |
| `all` | - | Yes | Yes | A U | |

### Party and matching

| Option | ABP | uBO | AdGuard | Lists | FOP |
|--------|-----|-----|---------|-------|-----|
| `third-party` | Yes | Yes | Yes | A E U | `3p` converts to it |
| `~third-party` | Yes | Yes | Yes | A E U | `1p` converts to it |
| `first-party` | - | Yes | - | A | Recognised; not converted |
| `1p` / `3p` | - | Yes | Alias | A U | Converted (see [below](#option-conversion)) |
| `strict1p` / `strict3p` | - | Yes | - | U | |
| `strict-first-party` / `strict-third-party` | - | - | Yes | | AdGuard's names for `strict1p` / `strict3p` |
| `match-case` | Yes | Yes | Yes | A E U | |
| `important` | - | Yes | Yes | A U | |
| `badfilter` | - | Yes | Yes | A U | |
| `cname` | - | Yes | - | U | |
| `network` | - | - | Yes | A | |
| `_` (`_____`, any length) | - | Yes | Yes | A U | The no-op modifier; recognised by shape |
| `noop` | - | - | Yes | | |
| `collapse` | Legacy | - | - | | Recognised |

### Exception and hiding switches

| Option | ABP | uBO | AdGuard | Lists | FOP |
|--------|-----|-----|---------|-------|-----|
| `elemhide` | Yes | Yes | Yes | A E | |
| `generichide` | Yes | Yes | Yes | A E U | `ghide` converts to it |
| `genericblock` | Yes | - | Yes | A E | |
| `specifichide` | - | Yes | Yes | | |
| `ehide` / `shide` | - | Yes | Alias | U | Recognised; not converted |
| `jsinject` | - | - | Yes | A | |
| `content` | - | - | Yes | A | |
| `extension` | - | - | Yes | A | Bare form. For the value form see `extension=`. |
| `stealth` | - | - | Yes | A | Bare form. For the value form see `stealth=`. |
| `urlblock` | - | - | Yes | A | Only on an `@@` exception |
| `empty` | - | Deprecated | Deprecated | | Recognised |
| `mp4` | - | Deprecated | Deprecated | | Recognised |

Some modifiers are valid **bare only on an `@@` exception**, where they switch
off every rule of that kind for the site. On a blocking rule the bare word is
missing its value, so it stays unknown there:
`urlblock`, `removeheader`, `replace`, `redirect`, `permissions`,
`urltransform`, `uritransform`, `urlskip`, `hls`, `jsonprune`, `xmlprune`,
`referrerpolicy`, `dnsrewrite`.

### Options with values

**Spaces kept** means FOP leaves whitespace in the value alone. For every
other network rule, FOP strips whitespace.

| Option | ABP | uBO | AdGuard | Lists | FOP |
|--------|-----|-----|---------|-------|-----|
| `domain=` | Yes | Yes | Yes | A E U | Entries sorted; `\|`-separated |
| `from=` | - | Yes | - | U | Converted to `domain=` |
| `to=` | - | Yes | Yes | A U | |
| `top=` | - | Yes | - | U | Restricts to the top-level context, as `to=` does the destination |
| `denyallow=` | - | Yes | Yes | A U | |
| `sitekey=` | Yes | - | - | E | |
| `csp=` | Yes | Yes | Yes | A E U | Spaces kept. Bare `csp` is recognised too. |
| `rewrite=` | Yes | - | - | E | `abp-resource:` values |
| `redirect=` | - | Yes | Yes | A U | |
| `redirect-rule=` | - | Yes | Yes | A U | |
| `removeparam=` | - | Yes | Yes | A U | Spaces kept; a `/regex/` value is left as written |
| `queryprune=` | - | Deprecated | - | | Old name for `removeparam=` |
| `removeheader=` | - | - | Yes | | |
| `replace=` | - | Yes | Yes | A U | Spaces kept |
| `header=` | Yes | Yes | Yes | A E U | Spaces kept |
| `addheader=` | Yes | - | - | E | Spaces kept |
| `requestheader=` | - | Yes | - | U | Spaces kept |
| `responseheader=` | - | - | - | | Recognised; spaces kept |
| `permissions=` | - | Yes | Yes | A U | Spaces kept |
| `referrerpolicy=` | - | - | Yes | A | |
| `method=` | - | Yes | Yes | A U | |
| `ipaddress=` | - | Yes | - | U | Spaces kept |
| `reason=` | - | Yes | Yes | U | Free text; spaces kept |
| `urlskip=` | - | Yes | - | U | Spaces kept |
| `uritransform=` | - | Yes | - | U | Spaces kept |
| `urltransform=` | - | Renamed | Yes | A | uBO calls it `uritransform=`; spaces kept |
| `jsonprune=` | - | - | Yes | | Spaces kept |
| `xmlprune=` | - | - | Yes | A | Spaces kept |
| `hls=` | - | - | Yes | A | |
| `cookie=` | - | - | Yes | A | Bare `cookie` is recognised too |
| `stealth=` | - | - | Yes | A | |
| `extension=` | - | - | Yes | A | Spaces kept, because the value names a userscript exactly (`'AdGuard Assistant'`) |
| `app=` | - | - | Yes | A | |
| `dnsrewrite=`, `dnstype=`, `client=`, `ctag=` | - | - | Yes (DNS) | | |
| `tag=` | - | - | - | | Recognised |

An escaped comma (`\,`) is part of an option's value, so FOP never splits an
option there. `$permissions=sync-xhr=()\,camera=()` is one option.

## Option conversion

uBO's short options are expanded by default. `--no-ubo-convert` turns this off.

| uBO | Converts to |
|-----|-------------|
| `xhr`, `xml` | `xmlhttprequest` |
| `css` | `stylesheet` |
| `frame`, `iframe` | `subdocument` |
| `doc` | `document` |
| `ghide` | `generichide` |
| `3p` | `third-party` |
| `1p` | `~third-party` |
| `~1p` | `third-party` |
| `from=` | `domain=` |

Negation is kept: `~xhr` becomes `~xmlhttprequest`. `first-party`,
`strict1p`, `strict3p`, `ehide`, `shide` and `all` are left as written.

### Selector and scriptlet conversion (opt-in)

| Flag | Converts | Notes |
|------|----------|-------|
| `--abp-convert` | `:-abp-has()` to `:has()`; `:-abp-contains()` to `:has-text()` | Separators are not touched. `:-abp-properties()` has no equivalent and is kept. |
| `--adguard-convert` | A `:has-text()` rule's `##` to `#?#`, and `#@#` to `#@?#` | AdGuard's spellings, so use it only for lists AdGuard reads. uBO HTML filters (`##^`) are skipped. |
| `--convert-trusted` | `trusted-set-cookie`, `trusted-set-local-storage-item` and `trusted-set-session-storage-item` to their non-trusted forms | Only when the value is one the non-trusted scriptlet accepts: a known keyword, or a number up to 32767. Covers uBO `+js()` and AdGuard `//scriptlet()`. |

## Cosmetic rule separators

**Default**, **`--parse-adguard`** and **`--alt-sort`** say whether FOP
treats the rule as cosmetic in that mode: domains sorted, selector tidied,
rules merged. A rule its mode does not cover is still sorted in place, but its
text is left as written.

| Separator | Purpose | ABP | uBO | AdGuard | Lists | Default | `--parse-adguard` | `--alt-sort` |
|-----------|---------|-----|-----|---------|-------|---------|-------------------|--------------|
| `##` | Element hiding | Yes | Yes | Yes | A E U | Yes | Yes | Yes |
| `#@#` | Element hiding exception | Yes | Yes | Yes | A E U | Yes | Yes | Yes |
| `#?#` | Extended CSS selectors | Yes | Yes | Yes | A E U | Yes | Yes | Yes |
| `#@?#` | Extended CSS exception | - | Yes | Yes | A U | Yes | Yes | Yes |
| `#$#` | ABP snippet, or AdGuard CSS injection | Snippets | - | CSS injection | A E | Yes* | Yes | Yes |
| `#@$#` | CSS injection exception | - | - | Yes | A | Yes* | Yes | Yes |
| `#%#` | JavaScript injection (AdGuard scriptlets) | - | - | Yes | A U | Yes* | Yes | Yes |
| `#@%#` | JavaScript injection exception | - | - | Yes | A | Yes* | Yes | Yes |
| `#$?#` | Extended CSS injection | - | - | Yes | A | - | Yes | - |
| `#@$?#` | Extended CSS injection exception | - | - | Yes | | - | Yes | - |
| `$$` | HTML filtering | - | - | Yes | A | - | Yes | - |
| `$@$` | HTML filtering exception | - | - | Yes | A | - | Yes | - |
| `##^` | HTML filtering (uBO) | - | Yes | - | U | Yes | Yes | Yes |

\* Default mode skips a rule with `{` or `}` after the separator, such as
AdGuard's CSS injection (`#$#body { overflow: auto; }`), and leaves it as
written. The addition checks tell the two `#$#` forms apart the same way: a
CSS injection carries a ` { ... }` block, and an ABP snippet does not.

`$$` counts as a separator only when the text before it could be a domain
list. So `$$` inside a URL (`/ad$$`) or after a network option is not one.

## Pseudo-classes

**Argument kept** means FOP leaves everything inside the parentheses alone.
Other pseudo-classes are tidied like any selector: spaces around `>`, `+` and
`~`, and pseudo-class names lowercased. An escaped colon (`\:`) or a regex
group (`(?:`) is never lowercased.

| Pseudo-class | ABP | uBO | AdGuard | Lists | FOP |
|--------------|-----|-----|---------|-------|-----|
| `:has()` | Yes | Yes | Yes | A E U | Argument kept |
| `:not()` | CSS | CSS | CSS | A E U | Tidied (its argument is a selector) |
| `:is()` | CSS | CSS | Yes | A E U | Tidied |
| `:has-text()` | Yes | Yes | Yes | E U | Argument kept; [merged](#sorting-and-merging) |
| `:contains()` | - | - | Yes | A | Argument kept |
| `:-abp-has()` | Yes | - | Yes | A E | Argument kept |
| `:-abp-contains()` | Yes | - | Yes | E | Argument kept |
| `:-abp-properties()` | Yes | - | - | | Argument kept |
| `:xpath()` | Yes | Yes | Yes | E U | Argument kept |
| `:upward()` | - | Yes | Yes | A U | Argument kept |
| `:matches-css()` | - | Yes | Yes | A U | Argument kept |
| `:matches-css-before()` / `:matches-css-after()` | - | Yes | - | U | Argument kept |
| `:matches-attr()` | - | Yes | Yes | U | Argument kept |
| `:matches-property()` | - | - | Yes | A | Argument kept |
| `:matches-path()` | - | Yes | - | U | Argument kept |
| `:matches-media()` | - | Yes | - | | Argument kept |
| `:matches-prop()` | - | Yes | - | | Argument kept |
| `:min-text-length()` | - | Yes | - | | Argument kept |
| `:watch-attr()` | - | Yes | - | | Argument kept |
| `:others()` | - | Yes | - | U | Argument kept |
| `:nth-ancestor()` | - | - | Yes | | Tidied (a number, so nothing changes) |
| `:shadow()` | - | - | - | U | Tidied (its argument is a selector) |
| `:style()` | - | Yes | - | U | Argument kept |
| `:remove()` | - | Yes | Yes | U | Argument kept |
| `:remove-attr()` | - | Yes | - | U | Argument kept |
| `:remove-class()` | - | Yes | - | U | Argument kept |

## Scriptlets, snippets and injection

| Syntax | Source | Lists | FOP |
|--------|--------|-------|-----|
| `##+js(name, args)` | uBO | U | One space after each comma that separates arguments: `+js(set,a,1)` becomes `+js(set, a, 1)`. An escaped comma (`\,`) is part of an argument and is left alone. An empty argument `,,` becomes `, ,`, which uBO reads the same because it trims arguments. Rules containing quotes are left as written. |
| `#%#//scriptlet('name', 'args')` | AdGuard | A | Left as written |
| `#%#` raw JavaScript | AdGuard | A | Left as written |
| `#$#snippet args; snippet args` | ABP | E | Left as written. Brackets inside a snippet's arguments are data, not selector syntax. |
| `#$#selector { style }` | AdGuard | A | Selector sorted as a cosmetic rule |
| `##^script:has-text(...)` | uBO | U | HTML filter; `:has-text()` merging applies |
| `##^responseheader(name)` | uBO | U | Left as written |
| `$$script[tag-content="..."]` | AdGuard | A | See [separators](#cosmetic-rule-separators) |
| `[$path=/regex/]domain##selector` | AdGuard | | Left as written |

Arguments that match literal text keep quotes, braces and parentheses that do
not balance. This applies to `+js()`, `:has-text()`, `:contains()`,
`:-abp-contains()`, `:-abp-properties()`, `:matches-*()`, `:xpath()` and
`:watch-attr()`, and the bracket check does not flag them.

## Domains

| Form | Example | FOP |
|------|---------|-----|
| Plain list | `b.com,a.com##.ad` | Lowercased and sorted: `a.com,b.com##.ad` |
| Exclusion | `~a.com` | Sorted beside its name, after the inclusion |
| Entity | `example.*` | Kept |
| uBO regex domain | `/^foo\.(?:com\|net)$/##.ad` | Kept. A regex ending `$/` is not an option marker. |
| Network `domain=` | `$domain=b.com\|a.com` | Sorted: `$domain=a.com\|b.com` |

## Directives and hints

| Line | Source | Lists | FOP |
|------|--------|-------|-----|
| `!#if condition` / `!#else` / `!#endif` | uBO, AdGuard | A U | Comment. Rules are never sorted across it, so a block keeps its rules. |
| `!#include file` | uBO, AdGuard | A U | Comment |
| `!#safari_cb_affinity(...)` | AdGuard | A | Comment |
| `%include file` | ABP | | Kept; ends a section |
| `!+ NOT_OPTIMIZED` | AdGuard | A | Hint: the rule after it stays in place |
| `!+ PLATFORM(...)` / `!+ NOT_PLATFORM(...)` | AdGuard | A | Hint: the rule after it stays in place |
| `!+ NOT_VALIDATE` | AdGuard | A | Hint: the rule after it stays in place |

A hint applies to the next rule only, so FOP keeps that rule directly below
it. The rule is tidied like any other, but it is not sorted with its section
or merged with its neighbours. Sorting would move a different rule under the
hint, and merging would widen the hint to other rules' domains. A chain of
hints, or a blank line after one, still targets the next rule. Any other
comment ends the hint. If FOP removes the hinted rule (a TLD-only rule, say),
the hint stays with whichever rule now follows it, and that rule is kept in
place instead.

## Whitespace

Whitespace is stripped from network rules, except:

- values of the options marked **Spaces kept** above;
- regex patterns (`/.../`), with or without options. `[^&=? ]` excludes a
  space; `[^&=?]` does not.

Cosmetic rules are trimmed at both ends, and selector combinators are spaced
(`a>b` becomes `a > b`) except inside an argument FOP keeps.

## Sorting and merging

A section is the run of rules between comments. Its first ten rules decide
how it is sorted: as cosmetic when more of them are cosmetic than network,
and as network otherwise (a tie counts as network). Only rules of the
section's kind are merged.

| Feature | What happens |
|---------|--------------|
| Duplicate removal | Identical rules within a section are removed |
| Option sorting | Network options sorted alphabetically (ignoring `~`), `domain=` last; its entries sorted. `$1p,xhr,from=b.com` becomes `$~third-party,xmlhttprequest,domain=b.com`. |
| Domain merging | Rules identical apart from their domains are merged: `a.com##.ad` + `b.com##.ad` becomes `a.com,b.com##.ad`. The same applies to network rules with `$domain=`. A list of exclusions only (`~a.com`) never merges with one that includes a domain, because that would change what it matches. |
| `:has-text()` merging | `##` rules with the same domains and base selector are merged into one regex: `:has-text(a)` + `:has-text(b)` becomes `:has-text(/a\|b/)`. This also covers uBO HTML filters (`##^`). |
| `:has-text()` regex flags | One flag set is kept: `/a/i` + `/b/i` or plain `b` becomes `/a\|b/i`. Two different flag sets are not merged. Neither is an empty alternative (`/foo\|/`), which would match everything. |
| Exceptions and other separators | `#@#`, `#@?#`, `#?#`, `#$#` and `#%#` rules are never `:has-text()`-merged. An exception cancels a rule by matching its exact text, so merging two would leave neither text to match. |
| `--pr-show-changes` | Lists the first 40 merges in the PR description and counts the rest |

Files are sorted in parallel, and a large merge group, such as one of
eyeo's Acceptable Ads rules with thousands of domains, is merged in one pass.

## Rules removed with a warning

| Rule | Example | Why |
|------|---------|-----|
| TLD only | `\|\|.com^`, `.net` | Blocks a whole top-level domain |
| Line starting with `"`, `)`, `]` or `}` | `"])` | Debris from a truncated rule; no valid rule starts this way |

A network rule whose domain holds no dot (`\|\|cfd^`, `\|\|countly-`,
`\|\|com/services/?rt=`) is **kept**: it is a whole-TLD or prefix match, which
is legitimate, and a typo looks the same. FOP mentions it once per run, which
`--ignore-dot-domains` silences.

The mention is only for a pattern that is nothing but the host, since that is
where a mistyped domain hides. Three shapes no typo can take stay quiet: a path
or wildcard under the host (`\|\|com/*/ModalEngage\|`), a host prefix ending in
`-` (`\|\|chamsocthe-`), and a host left to `ipaddress=`
(`\|\|cc^$doc,ipaddress=15.207.81.128`). The rule is kept in every case; only
the mention differs.
| Line under 3 characters | `a` | Counted in characters, not bytes. Unless `--ignore-line-minimum`. |

## Typo detection and fixing

Always on:

| Typo | Example | Fixed to |
|------|---------|----------|
| `.` between options | `$third-party.script` | `$script,third-party` (network rules only, never a line with a cosmetic separator) |

With `--fix-typos` (every line of every file), or `--fix-typos-on-add` (only
the lines a commit adds, fixed at a prompt or automatically with
`--auto-fix`):

| Typo | Example | Fixed to |
|------|---------|----------|
| Triple `$` | `\|\|ex.com$$$domain=a.com` | `\|\|ex.com$domain=a.com` |
| Double `$` | `\|\|ex.com$$domain=a.com` | `\|\|ex.com$domain=a.com` |
| Missing `$` | `\|\|ex.js^domain=a.com` | `\|\|ex.js^$domain=a.com` |
| Wrong domain separator | `domain=a.com,b.com` | `domain=a.com\|b.com` |
| Extra `#` | `a.com###.ad` | `a.com##.ad` |
| Single `#` | `a.com#.ad` | `a.com##.ad` |
| Double dot | `##..ad` | `##.ad` |
| Double comma | `a,,b##.ad` | `a,b##.ad` |
| Trailing comma | `a.com,##.ad` | `a.com##.ad` |
| Leading comma | `,a.com##.ad` | `a.com##.ad` |
| Space after comma | `a.com, b.com##.ad` | `a.com,b.com##.ad` |
| Wrong cosmetic separator | `a\|b##.ad` | `a,b##.ad` |

Fixes repeat (up to 9 passes) to catch typos revealed by an earlier fix.

## Checks on added rules

`--check-rules-on-add` checks the rules a commit adds. Defects fail `--ci`,
and `--remove-bad-rules` deletes them. Advice is only reported, because the
rule is legal as written.

| Finding | Kind | Example |
|---------|------|---------|
| Unknown option | Defect (with a suggestion) | `$scirpt` → did you mean `script`? A value-only option written bare (`$requestheader`) counts as unknown too. |
| Option with no value | Defect | `$domain=` |
| Empty option / option marker with nothing after it | Defect | `\|\|a.com^$script,`, `\|\|a.com^$` |
| Empty entry in an option value | Defect | `$domain=a.com\|\|b.com` |
| Separator with no selector | Defect | `a.com##` |
| Malformed domain list | Defect | `a.com,,b.com##.ad` |
| Unbalanced brackets in a selector | Defect | `##div[class="x"` (not checked in arguments that match text, or in ABP snippets) |
| Selector starts with a combinator | Defect | `##> .ad` |
| Space in the pattern | Advice | `\|\|exa mple.com^`. The sort removes the space anyway. |
| Bare domain | Advice | `example.com`. Did you mean `\|\|example.com^`? |
| Host rule with no `\|\|` anchor | Advice | `rbush.shop^`, which also matches `lampedburbush.shop` |
| Unanchored text that reads as nothing | Advice | `fdfdgfgdgfd^` |
| Not a domain -- a bare word matches any URL containing it | Only under `--remove-non-domain-on-add`, which reports and removes it. Nothing is said about such a line otherwise, by design: it is a legal substring rule | `isCookiesAccepted`. A leading or trailing `-` or `_` exempts the line, which is how a deliberate substring rule is written. |

## Known limitations

Found while checking the September 2026 snapshots:

- **Without `--parse-adguard`, AdGuard's `$$`, `#$?#` and CSS-injection
  rules are not sorted as cosmetic.** They are left exactly as written, so
  their domains are neither sorted nor merged. For a list AdGuard reads,
  that flag is the better match.
- **A merged domain list can grow very long.** In eyeo's
  `exceptionrules.txt`, lines kept under 20,000 characters merge into lines
  of up to 586k characters. Merging does not change what the rules match.
- **Sorting sections that mix network and cosmetic rules is not
  idempotent.** A second pass can reorder rules or merge further, but never
  changes what they match.
