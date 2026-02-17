# workbench

A terminal Kanban board for managing work sessions, with tmux and git worktree integration.

![workbench screenshot](screenshot.png)

## Dependencies

**Required:**
- [Rust](https://rustup.rs/) (for building)
- [tmux](https://github.com/tmux/tmux) (for terminal session management)
- [Claude CLI](https://github.com/anthropics/claude-code) (for AI-powered field filling)

## Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .
```

## Usage

Run `workbench` from any git repository:

```bash
cd ~/Code/my-project
workbench
```

The tool auto-detects the git repository root and uses it as the project identity. All sessions are scoped to that project.

### Git Worktrees

When you create a new session in a git repo, workbench automatically:
1. Creates a new branch (`wb/<session-name>`)
2. Creates a git worktree at `<repo>-<session-name>/`
3. Opens tmux sessions in the worktree directory

This lets you work on multiple branches simultaneously without stashing or switching.

### Keybindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `n` | New session |
| `e` | Edit session (name + custom fields) |
| `c` | View/add comments |
| `m` | Move session to different status |
| `d` | Delete session |
| `r` | Refresh |
| `s` | Settings (manage custom fields) |
| `x` | Clean up orphaned tmux sessions |
| `h/l` or arrows | Navigate columns |
| `j/k` or arrows | Navigate rows |
| `Enter` | Open/attach tmux session |
| `Space` | Peek at tmux pane content |
| `Esc` | Cancel/close |

### Session Indicators

- `$` Green prefix: tmux session is active
- `?` Yellow prefix: session is waiting for user input

### Custom Fields

Press `s` to open settings and define custom fields for your project. Each field has a name and description - the description helps the AI understand what to extract.

**Example setup for Linear tickets:**

| Field Name | Description |
|------------|-------------|
| Ticket ID | Linear ticket identifier like ABC-123 |
| Ticket URL | Full Linear ticket URL |

With these fields defined, you can paste a Linear URL like `https://linear.app/myteam/issue/ABC-123/fix-login-bug` in AI mode and it will automatically extract:
- **Ticket ID** → `ABC-123`
- **Ticket URL** → `https://linear.app/myteam/issue/ABC-123/fix-login-bug`

### AI Fill

When editing a session (`e`), press `Shift+Tab` to switch to AI mode. Paste or type your input (ticket URL, description, etc.) and press `Enter`. The AI parses your input and fills the matching fields based on their descriptions.

Under the hood, this runs the `claude` CLI with your input and field descriptions, returning a JSON array of extracted values. The terminal pane content is also included as context when available.

Requires the [Claude CLI](https://github.com/anthropics/claude-code) to be installed and authenticated (`claude` must be in your PATH).

## Data Storage

Sessions are stored in `~/.local/share/workbench/workbench.db` (SQLite).
