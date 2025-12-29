# Using FOP with Visual Studio Code

This guide covers Windows, macOS, and Linux.

## Quick Setup

Copy the [.vscode](/.vscode/) folder to your project root for pre-configured tasks and settings.

Or follow the manual setup below.

## Setup

### 1. Install FOP

**Via npm (recommended):**
```bash
npm install -g fop-cli
```

**Or download binary from [GitHub Releases](https://github.com/ryanbr/fop-rs/releases)**

### 2. Verify Installation

Open VS Code terminal:
- **Windows/Linux:** Press `` Ctrl+` ``
- **macOS:** Press `` Cmd+` ``

Then run:
```bash
fop --version
```

## Basic Usage

### Run FOP on Current Directory

1. Open your filter list repository in VS Code
2. Open terminal (`` Ctrl+` `` or `` Cmd+` ``)
3. Run:
```bash
fop --no-commit .
```

### Run FOP on Specific File
```bash
fop --check-file=easylist/easylist_general_block.txt --no-commit
```

## VS Code Tasks Integration

Copy `.vscode/tasks.json` from the [.vscode](/.vscode/) folder, or create your own.

**Available tasks:**

| Task | Description |
|------|-------------|
| FOP: Sort All | Sort all files (default build task) |
| FOP: Sort Current File | Sort currently open file |
| FOP: Sort and Commit | Sort all + git commit prompt |
| FOP: Preview Changes (Diff) | Preview changes without modifying |
| FOP: Fix Typos | Fix typos in all files |
| FOP: Fix Typos and Commit | Fix typos + git commit prompt |

**Run tasks:**
- **Windows/Linux:** `Ctrl+Shift+P` ? "Tasks: Run Task" ? Select FOP task
- **macOS:** `Cmd+Shift+P` ? "Tasks: Run Task" ? Select FOP task

Or use `Ctrl+Shift+B` / `Cmd+Shift+B` for the default task (FOP: Sort All).

## Keyboard Shortcuts

> **Note:** The default search shortcut (`Ctrl/Cmd+Shift+F`) is preserved. These examples use `Ctrl/Cmd+Alt+O`.

Add to `.vscode/keybindings.json` (user settings):

**Windows/Linux:**
```json
[
  {
    "key": "ctrl+alt+o",
    "command": "workbench.action.tasks.runTask",
    "args": "FOP: Sort All"
  },
  {
    "key": "ctrl+alt+shift+o",
    "command": "workbench.action.tasks.runTask",
    "args": "FOP: Sort Current File"
  }
]
```

**macOS:**
```json
[
  {
    "key": "cmd+alt+o",
    "command": "workbench.action.tasks.runTask",
    "args": "FOP: Sort All"
  },
  {
    "key": "cmd+alt+shift+o",
    "command": "workbench.action.tasks.runTask",
    "args": "FOP: Sort Current File"
  }
]
```

## Format on Save (Advanced)

Create `.vscode/settings.json`:
```json
{
  "files.associations": {
    "*.txt": "plaintext"
  },
  "editor.formatOnSave": false,
  "[plaintext]": {
    "editor.formatOnSave": false
  }
}
```

Then add a Run on Save extension task (requires "Run on Save" extension):
```json
{
  "emeraldwalk.runonsave": {
    "commands": [
      {
        "match": ".*\\.txt$",
        "cmd": "fop --check-file=${file} --no-commit --quiet"
      }
    ]
  }
}
```

## Project Configuration

Create `.fopconfig` in your project root:
```ini
# Skip commit prompt (use VS Code git integration instead)
no-commit = true

# Keep your formatting preferences
keep-empty-lines = false
backup = false

# Typo detection
fix-typos = true

# Quiet output
quiet = false
```

## Recommended Extensions

| Extension | Purpose |
|-----------|---------|
| [Adblock Syntax](https://marketplace.visualstudio.com/items?itemName=adguard.adblock) | Syntax highlighting for filter lists (ABP, uBO, AdGuard) |
| [Run on Save](https://marketplace.visualstudio.com/items?itemName=emeraldwalk.RunOnSave) | Auto-run FOP on save |
| [GitLens](https://marketplace.visualstudio.com/items?itemName=eamodio.gitlens) | Enhanced Git integration |

## Workflow Example

1. **Edit filter list** in VS Code
2. **Save file** (`Ctrl+S` / `Cmd+S`)
3. **Run FOP** (`Ctrl+Alt+O` / `Cmd+Alt+O` or via Tasks menu)
4. **Review changes** in Source Control panel (`Ctrl+Shift+G` / `Cmd+Shift+G`)
5. **Commit** using VS Code Git integration

## Troubleshooting

### "fop: command not found"

Add to your PATH or use full path in tasks.json:

**Linux:**
```json
"command": "/usr/local/bin/fop"
```

**macOS:**
```json
"command": "/usr/local/bin/fop"
```

**Windows:**
```json
"command": "C:\\Users\\YourName\\AppData\\Roaming\\npm\\fop.cmd"
```

### Task not showing

Reload VS Code window:
- **Windows/Linux:** `Ctrl+Shift+P` ? "Developer: Reload Window"
- **macOS:** `Cmd+Shift+P` ? "Developer: Reload Window"