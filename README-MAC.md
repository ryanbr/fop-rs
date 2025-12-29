# FOP for macOS

A guide to installing and using FOP (Filter Orderer and Preener) on macOS.

## Installation

### Option 1: Install via npm (Recommended)

This method automatically builds from source on macOS.

#### Step 1: Install Prerequisites

**Install Homebrew** (if not already installed):
```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

**Install Node.js**:
```bash
brew install node
```

**Install Rust**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Step 2: Fix npm Global Permissions (Recommended)

Avoid using `sudo` with npm:

```bash
mkdir -p ~/.npm-global
npm config set prefix '~/.npm-global'
echo 'export PATH=~/.npm-global/bin:$PATH' >> ~/.zshrc
source ~/.zshrc
```

#### Step 3: Install FOP

```bash
npm install -g fop-cli
```

This will build FOP from source automatically (~25 seconds).

#### Step 4: Verify Installation

```bash
fop --version
fop --help
```

### Option 2: Build from Source Manually

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone and build
git clone https://github.com/ryanbr/fop-rs.git
cd fop-rs
cargo build --release

# Binary will be at: target/release/fop
```

Add to PATH:
```bash
cp target/release/fop /usr/local/bin/
# Or add to ~/.zshrc:
echo 'export PATH="$HOME/fop-rs/target/release:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

## Usage

### Basic Commands

```bash
# Sort filters in current directory
fop

# Sort filters in specific directory
fop /path/to/easylist

# Sort without commit prompt
fop -n .

# Sort multiple directories
fop -n ~/easylist ~/fanboy
```

### All Options

```
-n, --no-commit       Skip repository commit prompt
--no-ubo-convert      Skip uBO to ABP option conversion
--no-msg-check        Skip commit message format validation
--disable-ignored     Process all files (ignore IGNORE_FILES/IGNORE_DIRS)
--no-sort             Skip sorting (only tidy and combine rules)
--alt-sort            Alternative sorting (by selector for all rule types)
--localhost           Sort hosts file entries (0.0.0.0/127.0.0.1)
--no-color            Disable colored output
--ignorefiles=        Additional files to ignore (comma-separated)
--ignoredirs=         Additional directories to ignore (comma-separated, partial names)
--config-file=        Custom config file path
--git-message=        Git commit message (skip interactive prompt)
-h, --help            Show help message
-V, --version         Show version number
```

### Examples

```bash
# Sort with custom ignore files
fop --ignorefiles=".json,.backup" -n .

# Sort hosts file
fop --localhost -n ~/hosts-files

# Use custom config file
fop --config-file=~/configs/my-fop.config -n .

# Auto-commit with message (no prompt)
fop --git-message="M: Fixed sorting" .
```

## VS Code Integration

See [Using FOP with Visual Studio Code](Using_FOP_with_Visual_Studio_Code.md) for tasks, keyboard shortcuts, and workflow tips.


## Configuration File

Create `.fopconfig` in your project or home directory:

```ini
# FOP Configuration
no-commit = false
no-ubo-convert = false
no-msg-check = false
disable-ignored = false
no-sort = false
alt-sort = false
localhost = false
no-color = false
ignorefiles = .json,.backup
```

Config file locations (checked in order):
1. `./.fopconfig` (current directory)
2. `~/.fopconfig` (home directory)

Command line arguments override config file settings.

## Troubleshooting

### "fop: command not found"

Ensure FOP is in your PATH:

```bash
# Check where fop is installed
which fop

# If using npm-global setup, ensure PATH is set
echo $PATH | grep npm-global

# Reload shell config
source ~/.zshrc
```

### "Rust/Cargo not found" during npm install

Install Rust and reload your shell:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
npm install -g fop-cli
```

### npm Permission Errors (EACCES)

Don't use `sudo`. Fix permissions instead:

```bash
mkdir -p ~/.npm-global
npm config set prefix '~/.npm-global'
echo 'export PATH=~/.npm-global/bin:$PATH' >> ~/.zshrc
source ~/.zshrc
```

### Build Errors

Ensure Xcode Command Line Tools are installed:

```bash
xcode-select --install
```

Update Rust:

```bash
rustup update
```

### "cargo build" is slow

First build downloads and compiles dependencies (~1-2 minutes). Subsequent builds are faster.

For faster debug builds during development:

```bash
cargo build        # Debug (faster compile)
cargo build --release  # Release (slower compile, faster runtime)
```

## Updating FOP

### Via npm

```bash
npm update -g fop-cli
```

### Via Source

```bash
cd fop-rs
git pull
cargo build --release
```

## Uninstalling

### npm installation

```bash
npm uninstall -g fop-cli
```

### Manual installation

```bash
rm /usr/local/bin/fop
# Or remove from wherever you placed it
```

## Apple Silicon (M1/M2/M3) Notes

FOP builds natively for Apple Silicon (arm64). The npm install automatically detects your architecture and builds the appropriate binary.

If you have issues, ensure you're using the native ARM version of Homebrew and Node:

```bash
# Check architecture
uname -m  # Should show: arm64

# Check Node architecture
node -p "process.arch"  # Should show: arm64
```

If using Rosetta (x86 emulation), performance will be slower. Install native ARM versions for best performance.
