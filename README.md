# FOP - Filter Orderer and Preener (Rust Edition)

A Rust port of the EasyList FOP tool for sorting and cleaning ad-blocking filter lists.

## Features

- **Filter sorting**: Alphabetically sorts blocking rules and element hiding rules
- **Domain combining**: Merges rules with identical selectors/patterns but different domains
- **Option normalization**: Converts uBO-specific options to standard ABP format
- **Wildcard cleanup**: Removes unnecessary wildcards from filters
- **Validation**: Removes invalid/overly-broad rules (TLD-only, too short, etc.)
- **Git/Hg integration**: Commit changes directly to repositories (can be disabled)
- **easylist_adservers.txt validation**: Ensures rules start with `|` or `/`

## Requirements

- Rust 1.75+ (install from https://rustup.rs)

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

# Sort multiple directories
fop -n ~/easylist ~/easyprivacy ~/fanboy-addon
```

## Command Line Options

| Option | Description |
|--------|-------------|
| `-n, --no-commit` | Just sort files, skip Git/Hg commit prompts |
| `--just-sort` | Alias for `--no-commit` |
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

## License

GPL-3.0 (same as original Python FOP)

## Credits

- Original Python FOP by Michael (EasyList project)
- Rust port maintains feature parity with Python version 3.9
