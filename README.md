# FOP - Filter Orderer and Preener (Rust Edition)

A Rust port of the EasyList FOP tool for sorting and cleaning ad-blocking filter lists.

## Features

- **Filter sorting**: Alphabetically sorts blocking rules and element hiding rules
- **Domain combining**: Merges rules with identical selectors/patterns but different domains
- **Option normalization**: Converts uBO-specific options to standard ABP format (can be disabled)
- **Wildcard cleanup**: Removes unnecessary wildcards from filters
- **Validation**: Removes invalid/overly-broad rules (TLD-only, too short, etc.)
- **Git integration**: Commit changes directly to repositories (can be disabled)
- **easylist_adservers.txt validation**: Ensures rules start with `|` or `/`
- **Parallel processing**: Uses all CPU cores for faster processing via Rayon

## Extended Syntax Support

FOP preserves extended filter syntax from various adblockers:

### uBlock Origin
- Scriptlet injection: `##+js(...)`, `##^script:has-text(...)`
- Procedural cosmetics: `:has-text()`, `:has()`, `:upward()`, `:remove()`, `:style()`, `:matches-css()`, `:xpath()`, `:remove-attr()`, `:remove-class()`, `:watch-attr()`, `:min-text-length()`, `:others()`
- Network options: `redirect=`, `redirect-rule=`, `removeparam=`, `denyallow=`, `replace=`, `header=`, `permissions=`, `to=`, `from=`, `method=`
- Regex domain rules: `/regex/##+js(...)`

### AdGuard
- Scriptlet injection: `#%#//scriptlet(...)`
- CSS injection: `#$#body { ... }`
- Exceptions: `#@%#`, `#@$#`

### Adblock Plus
- Extended selectors: `:-abp-has()`, `:-abp-contains()`, `:-abp-properties()`
- Action syntax: `{remove: true;}`, `{height:...}`, `{display:...}`
- Snippets: `#$#hide-if-contains`, `#$#simulate-mouse-event`

## Speed Comparison (FOP.py vs FOP Rust)

<img width="1850" height="855" alt="fop-graph" src="https://github.com/user-attachments/assets/ef9e1e24-5f40-4899-b21f-3e706261f5dd" />

## Installation

```bash
npm install -g fop-cli
```

### Building from source (optional)

Requires Rust 1.80+ (https://rustup.rs)

```bash
cd fop-rs
cargo build --release
cp target/release/fop /usr/local/bin/  # or add to PATH
```

## Usage

```bash
# Sort filters in current directory (with commit prompt)
fop

# Sort filters in a specific directory
fop /path/to/easylist

# Sort without commit prompt (like FOP-nocommit.py)
fop --no-commit /path/to/easylist
fop -n .

# Sort without converting uBO options to ABP format
fop --no-ubo-convert /path/to/ublock-filters

# Sort multiple directories
fop -n ~/easylist ~/easyprivacy ~/fanboy-addon
```

## Command Line Options

| Option | Description |
|--------|-------------|
| `-n, --no-commit` | Just sort files, skip Git commit prompts |
| `--just-sort` | Alias for `--no-commit` |
| `--no-ubo-convert` | Skip uBO to ABP option conversion (keep `xhr`, `3p`, `1p`, etc.) |
| `--no-msg-check` | Skip commit message format validation (M:/A:/P:) |
| `--disable-ignored` | Disable hardcoded ignored files and folders for testing |
| `--no-sort` | Don't sort rules, just combine |
| `--alt-sort` | More correct sorting method |
| `--localhost` | Sort hosts file entries (0.0.0.0/127.0.0.1 domain) |
| `--no-color` | Disable colored output |
| `--no-large-warning` | Disable large change warning prompt |
| `--backup` | Create .backup files before modifying |
| `--keep-empty-lines` | Keep empty lines in output |
| `--ignore-dot-domains` | Don't skip rules without dot in domain |
| `--ignorefiles=` | Additional files to ignore (comma-separated, partial names) |
| `--ignoredirs=` | Additional directories to ignore (comma-separated, partial names) |
| `--file-extensions=` | File extensions to process (default: .txt) |
| `--comments=` | Comment line prefixes (default: !) |
| `--disable-domain-limit=` | Files to skip short domain check (comma-separated) |
| `--warning-output=` | Output warnings to file instead of stderr |
| `--git-message=` | Git commit message (skip interactive prompt) |
| `--create-pr[=TITLE]` | Create PR branch instead of committing to current branch |
| `--git-pr-branch=NAME` | Base branch for PR (default: auto-detect main/master) |
| `--fix-typos` | Fix cosmetic rule typos in all files during sort |
| `--fix-typos-on-add` | Check cosmetic rule typos in git additions before commit |
| `--auto-fix` | Auto-fix typos without prompting (use with --fix-typos-on-add) |
| `--config-file=` | Custom config file path |
| `--show-config` | Show applied configuration and exit |
| `-h, --help` | Show help message |
| `-V, --version` | Show version number |

## Configuration File

Create `.fopconfig` in your working directory or home directory:

```ini
# Skip commit prompt
no-commit = false

# Skip uBO to ABP option conversion
no-ubo-convert = false

# Skip commit message format validation
no-msg-check = false

# Skip sorting (only tidy and combine rules)
no-sort = false

# Alternative sorting method
alt-sort = false

# Sort hosts file entries
localhost = false

# Disable colored output
no-color = false

# Disable large change warning prompt
no-large-warning = false

# Create .backup files before modifying
backup = false

# Keep empty lines in output
keep-empty-lines = false

# Don't skip rules without dot in domain
ignore-dot-domains = false

# Comment line prefixes
comments = !

# Files to skip short domain check
disable-domain-limit =

# Output warnings to file
warning-output =

# Additional files to ignore
ignorefiles = .json,.backup,.bak,.swp,.gz

# Additional directories to ignore
ignoredirs =

# File extensions to process
file-extensions = txt

# Create PR branch instead of committing
create-pr =

# Base branch for PR (default: auto-detect)
git-pr-branch =

# Fix cosmetic typos during sort
fix-typos = false

# Check typos in git additions
fix-typos-on-add = false

# Auto-fix without prompting
auto-fix = false
```

Command line arguments override config file settings.

## Platform Support

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `fop-*-linux-x86_64` |
| Linux x86_64 (AVX2) | `fop-*-linux-x86_64-v3` |
| macOS Intel | `fop-*-macos-x86_64` |
| macOS Apple Silicon | `fop-*-macos-arm64` |
| Windows x86_64 | `fop-*-windows-x86_64.exe` |
| Windows x86_64 (AVX2) | `fop-*-windows-x86_64-v3.exe` |

## Migrating from Python FOP

This Rust version is a drop-in replacement for both `FOP.py` and `FOP-nocommit.py`:

| Python | Rust Equivalent |
|--------|-----------------|
| `python3 FOP.py` | `fop` |
| `python3 FOP-nocommit.py` | `fop --no-commit` or `fop -n` |

## Performance

The Rust version is significantly faster than Python FOP due to:
- Compiled native code
- Parallel file processing with Rayon
- Optimized regex handling
- Efficient memory management

## License

GPL-3.0 (same as original Python FOP)

## Credits

- Original Python FOP by Michael (EasyList project)
- Rust port maintains feature parity with Python version 3.9
