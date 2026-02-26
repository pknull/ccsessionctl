# ccsessionctl

A TUI for managing Claude Code CLI sessions. Browse, search, preview, delete, and archive your Claude Code conversation history.

## Features

- **Interactive TUI** - Browse sessions with vim-style navigation
- **Session Preview** - View conversation content with syntax highlighting
- **Search & Filter** - Filter by project name, search within sessions
- **Bulk Operations** - Delete empty sessions, archive old conversations
- **Statistics** - View usage stats by project (session count, size, tokens)
- **Multiple Sort Options** - Sort by date, size, project, or name
- **Cross-Platform** - Works on Linux, macOS, and Windows

## Installation

### From Source

```bash
git clone https://github.com/pknull/ccsessionctl
cd ccsessionctl
cargo build --release
```

The binary will be at `target/release/ccsessionctl`.

### With Cargo

```bash
cargo install --git https://github.com/pknull/ccsessionctl
```

## Usage

### Interactive TUI

```bash
ccsessionctl
```

### CLI Options

```bash
ccsessionctl --list              # List sessions (non-interactive)
ccsessionctl --count             # Show session count only
ccsessionctl --stats             # Show usage statistics by project
ccsessionctl --prune-empty       # Delete all empty sessions
ccsessionctl --prune-empty --dry-run  # Preview what would be deleted
ccsessionctl -p myproject        # Filter by project name
ccsessionctl -s size             # Sort by size (date, size, project, name)
ccsessionctl -s date -r          # Sort by date, reversed
```

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | View session details |
| `d` | Delete selected session |
| `y` | Copy session content to clipboard |
| `/` | Search |
| `Esc` | Back / Cancel |
| `q` | Quit |
| `r` | Refresh session list |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `Ctrl+d` | Page down |
| `Ctrl+u` | Page up |

## Session Storage

Sessions are read from `~/.claude/projects/` where Claude Code stores conversation data.

## Requirements

- Rust 1.70+
- Claude Code CLI (for session data to exist)

## License

MIT
