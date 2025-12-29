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
| **Easylist Specifics Removed** | No Easylist hardcoded options |

## Quick Comparison

| Aspect | FOP.py | FOP.rs |
|--------|--------|--------|
| Speed | Slow (single-threaded) | Fast (parallel) |
| Configuration | Hardcoded | `.fopconfig` file |
| Syntax support | Mostly ABP (2015 rules) only | ABP, uBO, AdGuard |
| Scriptlet support | ✗ | ✓ |
| Typo fixing | ✗ | ✓ |
| PR workflow | ✗ | ✓ |
| Active development | ✗ | ✓ |
