# FOP for Windows

A guide to installing and using FOP (Filter Orderer and Preener) on Windows.

## Prerequisites

### Install Git (if not already installed)

Download from [https://git-scm.com/download/win](https://git-scm.com/download/win)

## Installation

### Option 1: Download Pre-built Binary (Recommended)

1. Go to [https://github.com/ryanbr/fop-rs/releases](https://github.com/ryanbr/fop-rs/releases)
2. Download the appropriate binary:
   - `fop-X.X.X-windows-x86_64.exe` - Works on all 64-bit Windows
   - `fop-X.X.X-windows-x86_64-v3.exe` - Faster, requires AVX2 (~2015+ CPUs)
3. Rename to `fop.exe` and place in a folder (e.g., `C:\Tools`)

### Option 2: Build from Source

Requires Rust: Download from [https://rustup.rs](https://rustup.rs)

```powershell
# Clone the repository
git clone https://github.com/ryanbr/fop-rs.git
cd fop-rs

# Build release version
cargo build --release

# Binary will be at: target\release\fop.exe
```

### Option 3: Install via npm

Requires Node.js: Download from [https://nodejs.org](https://nodejs.org)

```powershell
npm install -g fop-cli
```

## Usage

### Basic Commands

```powershell
# Sort filters in current directory
.\fop.exe

# Sort filters in specific directory
.\fop.exe C:\path\to\easylist

# Sort without commit prompt
.\fop.exe -n .

# Sort multiple directories
.\fop.exe -n C:\easylist C:\fanboy
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

```powershell
# Sort with custom ignore files
.\fop.exe --ignorefiles=".json,.backup" -n .

# Sort hosts file
.\fop.exe --localhost -n C:\hosts-files

# Use custom config file
.\fop.exe --config-file=C:\configs\my-fop.config -n .
```

## Add to PATH (Optional)

To run `fop` from anywhere:

1. Copy `fop.exe` to a folder (e.g., `C:\Tools`)
2. Add to PATH:
   - Press `Win + X` ? System ? Advanced system settings
   - Click "Environment Variables"
   - Under "User variables", select "Path" ? Edit
   - Click "New" ? Add `C:\Tools`
   - Click OK

Now you can run:
```powershell
fop -n C:\easylist
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
1. `.\.fopconfig` (current directory)
2. `%USERPROFILE%\.fopconfig` (home directory)

Command line arguments override config file settings.

## Troubleshooting

### "fop is not recognized"

- Ensure `fop.exe` is in your PATH, or use the full path:
  ```powershell
  C:\path\to\fop.exe -n .
  ```

### "cargo is not recognized"

- Restart your terminal after installing Rust
- Or run: `%USERPROFILE%\.cargo\bin\cargo`

### Build Errors

Ensure you have the latest Rust:
```powershell
rustup update
```

### Permission Denied

Run PowerShell as Administrator, or check file permissions on the filter lists.

### Colored Output Not Working

Windows Terminal and newer PowerShell versions support colors by default. If colors don't work:
1. Use Windows Terminal (recommended)
2. Use `--no-color` flag
3. Enable ANSI in PowerShell:
   ```powershell
   Set-ItemProperty HKCU:\Console VirtualTerminalLevel -Type DWORD 1
   ```

### Which Binary Should I Download?

| Binary | Compatibility | Performance |
|--------|---------------|-------------|
| `fop-X.X.X-windows-x86_64.exe` | All 64-bit Windows | Standard |
| `fop-X.X.X-windows-x86_64-v3.exe` | Windows with AVX2 (~2015+ CPUs) | ~10-20% faster |

**How to check if your CPU supports AVX2:**

```powershell
# In PowerShell
(Get-WmiObject Win32_Processor).Caption
```

If your CPU is Intel Haswell (2013+) or AMD Excavator (2015+) or newer, use the `-v3` version.

## Building from Source (Development)

```powershell
# Clone
git clone https://github.com/ryanbr/fop-rs.git
cd fop-rs

# Build debug version (faster compile, slower runtime)
cargo build

# Build release version (slower compile, faster runtime)
cargo build --release

# Run tests
cargo test

# Run directly without building separately
cargo run -- -n C:\easylist
```