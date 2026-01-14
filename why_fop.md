# FOP - Filter Orderer and Preener

## What is FOP?

FOP is a tool that allows adblock list authors to quickly make list additions and changes, then commit to Git. It automatically sorts & combines domains/rules and cleans up unnecessary spacing. Rules are sorted between `!` comment sections.

## Why FOP.rs over FOP.py?

### Problems with FOP.py

- **Outdated**: Built for 2015-era extensions and features. New adblock syntax wasn't supported - incompatible rules were hardcoded to be ignored.
- **Unmaintained**: No active development since 2015, only minor fixes.
- **EasyList-specific**: Tuned specifically for EasyList without consideration for other lists' features or preferences.
- **No configuration**: Everything hardcoded in FOP.py. Any customization required editing the source code.
- **Slow**: Single-threaded Python performance. Slower performance as you sort more files.
- **Poor Git integration**: Basic functionality.

### Benefits of FOP.rs

| Feature | Description |
|---------|-------------|
| **Fast** | Rust-based with parallel processing via Rayon |
| **Configurable** | User `.fopconfig` file for preferences |
| **Typo detection** | Detects and auto-fixes common filter typos |
| **Flexible formatting** | Option to preserve empty lines |
| **Multi-syntax support** | Compatible with ABP, uBO, and AdGuard rules |
| **Syntax conversion** | Convert between uBO & ABP network rule options |
| **Clean Git history** | Auto-rebase to avoid "Merge branch 'master'" commits |
| **PR workflow** | Create Git PR branches instead of direct commits |
| **Access control** | Allow specific users direct push access |
| **Hosts file support** | Can sort localhost-based adblock rules (0.0.0.0/127.0.0.1) |
| **Dry-run mode** | Sort without committing (`--no-commit`) |
| **Flexible commit messages** | No hardcoded A:/P:/M: format requirement (`--no-msg-check`) |
| **Single file processing** | Process individual files (`--check-file`) |
| **Diff preview** | Output changes as diff without modifying files (`--output-diff`) |
| **CI-friendly** | Quiet mode for automated pipelines (`--quiet`) |

### Quick Comparison

| Aspect | FOP.py | FOP.rs |
|--------|--------|--------|
| Speed | Slow (single-threaded) | Fast (parallel) |
| Configuration | Hardcoded | `.fopconfig` file |
| Syntax support | Mostly ABP only (2015) | ABP, uBO, AdGuard |
| Typo fixing | No | Yes |
| PR workflow | No | Yes |
| Active development | No | Yes |

## FOP Processing Actions

### Rule Sorting & Deduplication

| Action | Before | After |
|--------|--------|-------|
| Sort alphabetically | `b.com##.ad`<br>`a.com##.ad` | `a.com##.ad`<br>`b.com##.ad` |
| Remove duplicates | `example.com##.ad`<br>`example.com##.ad` | `example.com##.ad` |

### Rule Combining

| Action | Before | After |
|--------|--------|-------|
| Combine domains (element hiding) | `a.com##.ad`<br>`b.com##.ad` | `a.com,b.com##.ad` |
| Combine domains (blocking) | `||x.js$domain=a.com`<br>`||x.js$domain=b.com` | `||x.js$domain=a.com\|b.com` |
| Merge :has-text() rules | `a.com##div:has-text(foo)`<br>`a.com##div:has-text(bar)` | `a.com##div:has-text(/foo\|bar/)` |

### Cleanup & Normalization

| Action | Before | After |
|--------|--------|-------|
| Remove trailing wildcards | `||example.com^*` | `||example.com^` |
| Lowercase domains | `Example.COM##.ad` | `example.com##.ad` |
| Trim whitespace | `  example.com##.ad  ` | `example.com##.ad` |

### Validation & Removal

| Action | Before | After |
|--------|--------|-------|
| Remove TLD-only rules | `||com^` | *(removed)* |
| Remove short rules (<3 chars) | `ab` | *(removed)* |
| Validate adservers.txt | `example.com^` | *(warning: must start with \| or /)* |

### Optional Conversions

| Action | Before | After | Flag |
|--------|--------|-------|------|
| uBO → ABP options | `||example.com$xhr,3p` | `||example.com$xmlhttprequest,third-party` | *(default)* |
| Fix typos | `##.addvertisement` | `##.advertisement` | `--fix-typos` |

### Preserved Unchanged

- Comments (`! comment`)
- Preprocessor directives (`!#if`, `!#endif`)
- Extended syntax (scriptlets `##+js()`, CSS injection `#$#`)
- Regex domain rules (`/regex/##+js(...)`)
- AdGuard syntax (`#%#//scriptlet(...)`)