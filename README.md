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

## Requirements

- Rust 1.80+ (install from https://rustup.rs)

## Building

```bash
# Clone or download the source
cd fop-rs

# Build release binary
make

# Or using cargo directly
cargo build --release
```

## Installation

### From npm (recommended)

```bash
npm install -g fop-cli
```

### From source

```bash
# Install to /usr/local/bin (may need sudo)
make install

# Or install to custom location
make PREFIX=~/.local install

# Or manually copy the binary
cp target/release/fop /usr/local/bin/
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
| `--no-sort` | Don't sort rulse, just combine |
| `--alt-sort` | More correct sorting method |
| `-h, --help` | Show help message |
| `-V, --version` | Show version number |

## Makefile Targets

| Target      | Description                          |
|-------------|--------------------------------------|
| `make`      | Build release binary (default)       |
| `make debug`| Build debug binary                   |
| `make test` | Run tests                            |
| `make install` | Install to /usr/local/bin         |
| `make uninstall` | Remove from /usr/local/bin      |
| `make clean`| Remove build artifacts               |
| `make dist` | Create distributable archive         |
| `make info` | Show system and Rust info            |
| `make help` | Show all available targets           |

## Platform Support

- Linux x86_64
- Linux ARM64
- macOS x86_64 (Intel)
- macOS ARM64 (Apple Silicon)

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
