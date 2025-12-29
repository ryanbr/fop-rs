# Using FOP with Visual Studio Code

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

Create `.vscode/tasks.json` in your project:
```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "FOP: Sort All",
      "type": "shell",
      "command": "fop",
      "args": ["--no-commit", "."],
      "group": "build",
      "presentation": {
        "echo": true,
        "reveal": "always",
        "panel": "shared"
      }
    },
    {
      "label": "FOP: Sort Current File",
      "type": "shell",
      "command": "fop",
      "args": ["--check-file=${file}", "--no-commit"],
      "group": "build"
    },
    {
      "label": "FOP: Sort and Commit",
      "type": "shell",
      "command": "fop",
      "args": ["."],
      "group": "build"
    },
    {
      "label": "FOP: Preview Changes (Diff)",
      "type": "shell",
      "command": "fop",
      "args": ["--no-commit", "--output-diff=changes.diff", "."],
      "group": "build"
    },
    {
      "label": "FOP: Fix Typos",
      "type": "shell",
      "command": "fop",
      "args": ["--fix-typos", "--no-commit", "."],
      "group": "build"
    }
  ]
}
```

**Run tasks:**
- **Windows/Linux:** `Ctrl+Shift+P` ? "Tasks: Run Task" ? Select FOP task
- **macOS:** `Cmd+Shift+P` ? "Tasks: Run Task" ? Select FOP task

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
| [Filter List Syntax](https://marketplace.visualstudio.com/items?itemName=piquark6046.filter-list) | Syntax highlighting for filter lists |
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