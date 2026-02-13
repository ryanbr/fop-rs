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
- **:has-text() merging**: Combines rules with same base selector into single regex
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
<img width="2052" height="1028" alt="foprs" src="https://github.com/user-attachments/assets/7c746d17-23dc-4ba7-8d6f-f3d168c93d4a" />

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
| `--ignore-all-but=` |  Only process these files, ignore all others (comma-separated) |
| `--file-extensions=` | File extensions to process (default: .txt) |
| `--comments=` | Comment line prefixes (default: !) |
| `--warning-output=` | Output warnings to file instead of stderr |
| `--git-message=` | Git commit message (skip interactive prompt) |
| `--history=` | Predefined commit messages for arrow key selection (comma-separated) |
| `--create-pr[=TITLE]` | Create PR branch instead of committing to current branch |
| `--git-pr-branch=NAME` | Base branch for PR (default: auto-detect main/master) |
| `--pr-show-changes` | Include rule changes (combines, merges, typos) in PR body |
| `--fix-typos` | Fix cosmetic rule typos in all files during sort |
| `--fix-typos-on-add` | Check cosmetic rule typos in git additions before commit |
| `--auto-fix` | Auto-fix typos without prompting (use with --fix-typos-on-add) |
| `--only-sort-changed` | Only process files changed according to git |
| `--check-banned-list=FILE` | Check for banned domains in git additions |
| `--auto-banned-remove` | Auto-remove banned domains and commit |
| `--ci` | CI mode - exit with error code on failures (banned domains) |
| `--rebase-on-fail` | Auto rebase and retry push if it fails |
| `--ignore-config` | Ignore .fopconfig file, use only CLI args |
| `--output` | Output changed files with --changed suffix (no overwrite) |
| `--check-file=FILE` | Process a single file | 
| `--output-diff=FILE` | Output changes as diff (no files modified) | 
| `--quiet` | Limit console output, less verbose |
| `--limited-quiet` | Suppress directory listing only |
| `--add-timestamp=FILES` | Update timestamp for specific files only (comma-separated) |
| `--validate-checksum=FILES` | Validate checksum for specific files (exit 1 on failure) |
| `--validate-checksum-and-fix=FILES` | Validate and fix invalid checksums |
| `--add-checksum=FILES` | Add/update checksum for specific files (comma-separated) |
| `--add-timestamp` | Update timestamp in file header (Last Modified/Last Updated) |
| `--config-file=` | Custom config file path |
| `--show-config` | Show applied configuration and exit |
| `--git-binary=<path>` | Path to git binary (default: git in PATH) |
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

# Include rule changes in PR body
pr-show-changes = false

# Base branch for PR (default: auto-detect)
git-pr-branch =

# Fix cosmetic typos during sort
fix-typos = false

# Check typos in git additions
fix-typos-on-add = false

# Auto-fix without prompting
auto-fix = false

# Only sort git-changed files (skip unchanged)
only-sort-changed = false

# Path to banned domain list file
check-banned-list =

# Auto-remove banned domains and commit
auto-banned-remove = false

# CI mode - exit with error code on failures
ci = false

# Auto rebase and retry if push fails
rebase-on-fail = false

# Suppress most output (for CI)
quiet = false

# Users allowed to push directly when create-pr is enabled (comma-separated, case-insensitive)
direct-push-users =

# Predefined commit messages for arrow key selection (comma-separated)
# Use up/down arrows at commit prompt to cycle through these
history = A: ,P: ,M: Update,M: Cleanup,M: Sort,M: Adjust
```

Command line arguments override config file settings.

## Platform Support

### Pre-built Binaries

| Platform | Binary | Optimization | Compatible Devices |
|----------|--------|--------------|-------------------|
| **Linux** | | | |
| x86_64 | `linux-x86_64` | Baseline | All 64-bit Intel/AMD |
| x86_64 | `linux-x86_64-v3` | AVX2 | Intel Haswell+ / AMD Excavator+ (~2015+) |
| x86 | `linux-x86_32` | Baseline | 32-bit systems, older hardware |
| ARM64 | `linux-arm64` | Baseline | Raspberry Pi 3/4/5, Orange Pi 3/4/5, all ARM64 |
| ARM64 | `linux-arm64-n1` | Neoverse N1 | Pi 5, Orange Pi 5, AWS Graviton2+, Ampere Altra |
| RISC-V | `linux-riscv64` | Baseline | SiFive, StarFive VisionFive 2, Milk-V |
| **macOS** | | | |
| Intel | `macos-x86_64` | Baseline | All Intel Macs |
| Apple Silicon | `macos-arm64` | Apple M1 | M1, M2, M3, M4, M5 Macs |
| **Windows** | | | |
| x86_64 | `windows-x86_64.exe` | Baseline | All 64-bit Windows |
| x86_64 | `windows-x86_64-v3.exe` | AVX2 | Intel Haswell+ / AMD Excavator+ (~2015+) |
| x86 | `windows-x86_32.exe` | Baseline | 32-bit Windows |
| ARM64 | `windows-arm64-v2.exe` | Cortex-A78 | Surface Pro X, Snapdragon laptops |

### Which binary should I use?

**Linux/Windows x86_64:**
- Use `-v3` for CPUs from ~2015+ (Haswell, Ryzen) - ~10-20% faster due to AVX2
- Use baseline if unsure or on older CPUs

**Linux ARM64 (Raspberry Pi / Orange Pi):**
- Use `linux-arm64` for Pi 3, Pi 4, Orange Pi 3/4, or if unsure
- Use `linux-arm64-n1` for Pi 5, Orange Pi 5, AWS Graviton2+ (~10-20% faster)

**npm install** downloads baseline binaries for maximum compatibility. Optimized versions are available from [GitHub Releases](https://github.com/ryanbr/fop-rs/releases).

### Build Details

| Binary | Target | RUSTFLAGS |
|--------|--------|-----------|
| `linux-x86_64` | Native | - |
| `linux-x86_64-v3` | Native | `-C target-cpu=x86-64-v3` |
| `linux-x86_32` | `i686-unknown-linux-gnu` | - |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | - |
| `linux-arm64-n1` | `aarch64-unknown-linux-gnu` | `-C target-cpu=neoverse-n1` |
| `linux-riscv64` | `riscv64gc-unknown-linux-gnu` | - |
| `macos-x86_64` | `x86_64-apple-darwin` | - |
| `macos-arm64` | `aarch64-apple-darwin` | `-C target-cpu=apple-m1` |
| `windows-x86_64.exe` | `x86_64-pc-windows-gnu` | - |
| `windows-x86_64-v3.exe` | `x86_64-pc-windows-gnu` | `-C target-cpu=x86-64-v3` |
| `windows-x86_32.exe` | `i686-pc-windows-gnu` | - |
| `windows-arm64-v2.exe` | `aarch64-pc-windows-msvc` | `-C target-cpu=cortex-a78` |


**Unsupported platform?**
FOP.rs builds from source on any platform with Rust 1.80+:
```bash
cargo build --release
```

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

## Windows PowerShell Note

If you see `UnauthorizedAccess` or `PSSecurityException` when running `fop` in PowerShell, run this once to fix permanently:
```powershell
Set-ExecutionPolicy RemoteSigned -Scope CurrentUser
```

Alternatively, use Git Bash, MINGW64, or WSL.

## License

GPL-3.0 (same as original Python FOP)

## Credits

- Original Python FOP by Michael (EasyList project)
- Rust port maintains feature parity with Python version 3.9
