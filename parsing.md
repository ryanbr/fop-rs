# FOP Filter Parsing Reference

## Network Filter Options

### Recognised Options

| Option | ABP | uBO | AdGuard | Notes |
|--------|-----|-----|---------|-------|
| `script` | Yes | Yes | Yes | |
| `image` | Yes | Yes | Yes | |
| `stylesheet` | Yes | Yes | Yes | |
| `font` | Yes | Yes | Yes | |
| `media` | Yes | Yes | Yes | |
| `popup` | Yes | Yes | Yes | |
| `document` | Yes | Yes | Yes | |
| `subdocument` | Yes | Yes | Yes | |
| `xmlhttprequest` | Yes | Yes | Yes | |
| `websocket` | Yes | Yes | Yes | |
| `webrtc` | Yes | Yes | Yes | |
| `ping` | Yes | Yes | Yes | |
| `object` | Yes | Yes | Yes | |
| `object-subrequest` | Yes | - | - | Legacy |
| `other` | Yes | Yes | Yes | |
| `third-party` | Yes | Yes | Yes | |
| `match-case` | Yes | Yes | Yes | |
| `collapse` | Yes | Yes | - | |
| `elemhide` | Yes | Yes | Yes | |
| `generichide` | Yes | Yes | Yes | |
| `genericblock` | Yes | - | - | |
| `important` | - | Yes | Yes | |
| `badfilter` | - | Yes | Yes | |
| `all` | - | Yes | Yes | |
| `popunder` | - | - | Yes | |
| `empty` | - | Yes | - | |
| `cname` | - | Yes | - | |
| `inline-script` | - | Yes | - | |
| `network` | - | - | Yes | |
| `content` | - | - | Yes | |
| `extension` | - | - | Yes | |
| `jsinject` | - | - | Yes | |
| `stealth` | - | - | Yes | |
| `cookie` | - | - | Yes | |

### Recognised Options with Values

| Option | ABP | uBO | AdGuard | Notes |
|--------|-----|-----|---------|-------|
| `domain=` | Yes | Yes | Yes | Pipe-separated domains |
| `csp=` | Yes | Yes | Yes | Content Security Policy |
| `redirect=` | - | Yes | Yes | |
| `redirect-rule=` | - | Yes | - | |
| `rewrite=` | Yes | - | - | ABP resource rewrite |
| `replace=` | - | - | Yes | |
| `removeparam=` | - | Yes | Yes | |
| `removeheader=` | - | Yes | Yes | |
| `addheader=` | - | - | Yes | |
| `responseheader=` | - | Yes | - | |
| `header=` | - | Yes | - | |
| `permissions=` | - | Yes | - | |
| `referrerpolicy=` | - | Yes | - | |
| `jsonprune=` | - | - | Yes | Commas in value preserved |
| `denyallow=` | - | Yes | Yes | |
| `to=` | - | Yes | - | |
| `from=` | - | Yes | - | Converted to `domain=` |
| `method=` | - | Yes | Yes | |
| `sitekey=` | Yes | - | Yes | |
| `app=` | - | - | Yes | |
| `ipaddress=` | - | - | Yes | |
| `urltransform=` | - | Yes | - | |
| `uritransform=` | - | Yes | - | |
| `urlskip=` | - | Yes | - | |
| `reason=` | - | - | Yes | |
| `cookie=` | - | - | Yes | |

## uBO Short Option Conversion

When `--convert-ubo` is enabled (default), short options are expanded:

| uBO Short | Converts To |
|-----------|-------------|
| `xhr` | `xmlhttprequest` |
| `css` | `stylesheet` |
| `1p` | `~third-party` |
| `3p` | `third-party` |
| `frame` | `subdocument` |
| `iframe` | `subdocument` |
| `doc` | `document` |
| `ghide` | `generichide` |
| `xml` | `xmlhttprequest` |
| `from=` | `domain=` |

Negation (`~`) is preserved (e.g., `~xhr` becomes `~xmlhttprequest`).

## ABP Conversion

When `--abp-convert` is enabled:

| ABP Syntax | Converts To |
|-----------|-------------|
| `:-abp-has()` | `:has()` |
| `:-abp-contains()` | `:has-text()` |
| `#?#` (with `:-abp-has` only) | `##` |

`:-abp-properties()` is preserved (no equivalent).

## Cosmetic Rule Separators

| Separator | Source | Parsed | Sorted | Notes |
|-----------|--------|--------|--------|-------|
| `##` | ABP/uBO/AdGuard | Yes | Yes | Standard element hiding |
| `#@#` | ABP/uBO/AdGuard | Yes | Yes | Element hiding exception |
| `#?#` | uBO/AdGuard | Yes | Yes | Extended CSS selectors |
| `#@?#` | uBO/AdGuard | Yes | Yes | Extended CSS exception |
| `#$#` | AdGuard | Yes | Yes | CSS injection / scriptlet |
| `#@$#` | AdGuard | Yes | Yes | CSS injection exception |
| `#$?#` | AdGuard | Yes | Yes | Extended CSS injection |
| `#@$?#` | AdGuard | Yes | Yes | Extended CSS injection exception |
| `#%#` | AdGuard | Yes | Yes | JavaScript injection |
| `#@%#` | AdGuard | Yes | Yes | JavaScript injection exception |
| `$$` | AdGuard | Yes* | Yes* | HTML filtering (*requires `--parse-adguard`) |
| `$@$` | AdGuard | Yes* | Yes* | HTML filtering exception (*requires `--parse-adguard`) |

## Extended CSS Pseudo-Classes

These pseudo-classes are recognised and preserved without modification:

| Pseudo-Class | Source | Notes |
|-------------|--------|-------|
| `:has()` | CSS/uBO/AdGuard | Native CSS, preserved |
| `:not()` | CSS/uBO/AdGuard | Native CSS, preserved |
| `:is()` | CSS | Native CSS, preserved |
| `:where()` | CSS | Native CSS, preserved |
| `:has-text()` | uBO/AdGuard | Text content matching |
| `:style()` | uBO/AdGuard | Inline style injection |
| `:remove()` | uBO | Element removal |
| `:remove-attr()` | uBO/AdGuard | Attribute removal |
| `:remove-class()` | uBO/AdGuard | Class removal |
| `:matches-path()` | uBO | URL path matching |
| `:matches-css()` | uBO/AdGuard | CSS property matching |
| `:matches-css-before()` | uBO/AdGuard | ::before CSS matching |
| `:matches-css-after()` | uBO/AdGuard | ::after CSS matching |
| `:matches-media()` | uBO | Media query matching |
| `:matches-prop()` | uBO | CSS property pattern |
| `:upward()` | uBO/AdGuard | Ancestor selection |
| `:xpath()` | uBO/AdGuard | XPath selection |
| `:watch-attr()` | uBO | Attribute observer |
| `:min-text-length()` | uBO/AdGuard | Minimum text length |
| `:-abp-has()` | ABP | ABP has matching |
| `:-abp-contains()` | ABP | ABP text matching |
| `:-abp-properties()` | ABP | ABP CSS property |

## Procedural / Scriptlet Syntax

| Syntax | Source | Handled |
|--------|--------|---------|
| `+js(scriptlet, args)` | uBO/AdGuard | Preserved, spacing normalised |
| `//scriptlet(name, args)` | AdGuard | Preserved |
| `[$path=/regex/]domain##selector` | AdGuard | Passed through unchanged |

## Rules Ignored / Skipped

| Rule Type | Behaviour |
|-----------|-----------|
| Comments (`!`) | Preserved, not sorted |
| Section headers (`[...]`) | Preserved, not sorted |
| `%include` directives | Preserved, not sorted |
| Lines < 3 characters | Skipped |
| `[$path=...]` cosmetic modifiers | Passed through unchanged |
| Regex value options (`=/.../ `) | Returned unchanged |
| Empty lines | Removed (unless `--keep-empty-lines`) |

## Rules Removed with Warning

| Pattern | Example | Reason |
|---------|---------|--------|
| TLD-only rules | `\|\|.com^`, `.net` | Overly broad |
| Domain without dot | `\|\|click^$script` | Invalid domain (unless `--ignore-dot-domains`) |

## Typo Detection and Fixing

When `--fix-typos` is enabled:

### Network Rule Typos

| Typo | Example | Corrected To |
|------|---------|-------------|
| Triple `$` | `\|\|ex.com$$$script` | `\|\|ex.com$script` |
| Double `$` | `\|\|ex.com$$script` | `\|\|ex.com$script` |
| Missing `$` | `\|\|ex.js^domain=a.com` | `\|\|ex.js^$domain=a.com` |
| Wrong domain separator | `domain=a.com,b.com` | `domain=a.com\|b.com` |

### Cosmetic Rule Typos

| Typo | Example | Corrected To |
|------|---------|-------------|
| Extra `#` | `domain###.ad` | `domain##.ad` |
| Single `#` | `domain#.ad` | `domain##.ad` |
| Double dot | `##..ad` | `##.ad` |
| Double/triple comma | `a,,b##.ad` | `a,b##.ad` |
| Trailing comma | `a.com,##.ad` | `a.com##.ad` |
| Leading comma | `,a.com##.ad` | `a.com##.ad` |
| Space after comma | `a.com, b.com##.ad` | `a.com,b.com##.ad` |
| Wrong cosmetic separator | `a\|b##.ad` | `a,b##.ad` |

Typos are fixed iteratively (up to 9 passes) to handle cascading corrections.

## Rule Merging

| Feature | Description |
|---------|-------------|
| `:has-text()` merging | Rules with same base selector and domain combined into regex: `a,b` becomes `/a\|b/` |
| Duplicate removal | Identical rules within a section are deduplicated |
| Domain sorting | Domains in `domain=` and cosmetic domain lists sorted alphabetically |
| Option sorting | Network filter options sorted alphabetically |
