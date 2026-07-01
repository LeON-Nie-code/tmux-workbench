# ws

`ws` is a remote workspace memory manager for terminal-based development.

It indexes workspaces across SSH servers and tmux sessions, stores the useful
context locally, and gives you one command or TUI to jump back into a project.
It can index both local tmux sessions and tmux sessions on SSH servers.

## Concept

A workspace is not just a tmux session. It is:

- server
- local or SSH connection type
- SSH command
- tmux session
- project path
- running agent or shell
- pane snapshot
- git branch, head, dirty state, ahead/behind
- status
- notes

The stable workspace ID is:

```text
<server>/<tmux-session>
```

This avoids collisions when two servers have a session with the same name.

## Stack

- Rust
- clap for CLI parsing
- ratatui + crossterm for TUI
- rusqlite for the local index
- system `ssh` and remote `tmux` for discovery and attach

Using system `ssh` keeps support for existing `~/.ssh/config`, keys,
ProxyCommand, and generated GCloud SSH hosts.

## Usage

Create the default config:

```bash
cargo run -- init
```

Scan all configured servers:

```bash
cargo run -- scan
```

`scan` includes a built-in `local` server for tmux sessions on the current
machine. SSH servers come from `~/.config/ws/config.yaml`.

List indexed workspaces:

```bash
cargo run -- list
```

When the workspace path is inside a Git repository, `list`, TUI detail, and
JSON output include branch, HEAD, dirty state, and upstream ahead/behind counts.

Filter or export indexed workspaces:

```bash
cargo run -- list --server AI-Teacher-Baidu
cargo run -- list --status active
cargo run -- list --all
cargo run -- list --json
```

Attach to a workspace:

```bash
cargo run -- attach AI-Teacher-Baidu/NeuroPlay
```

Recreate a missing tmux session from the indexed path:

```bash
cargo run -- recreate AI-Teacher-Baidu/NeuroPlay
```

Check SSH, tmux, and indexed session health:

```bash
cargo run -- doctor
```

Edit the config:

```bash
cargo run -- open-config
```

Add a note:

```bash
cargo run -- note AI-Teacher-Baidu/NeuroPlay "Frontend is in frontend; backend uses uv."
```

Set a display alias and tags:

```bash
cargo run -- alias AI-Teacher-Baidu/NeuroPlay neuro
cargo run -- tags AI-Teacher-Baidu/NeuroPlay research frontend
```

Open the TUI:

```bash
cargo run
```

TUI shortcuts:

```text
Enter  attach
/      search by name, server, path, agent, note, or pane
n      edit note in $EDITOR
a      archive or unarchive workspace
z      show or hide archived workspaces
r      rescan servers
j/k    move
q      quit
```

The left column shows workspace, server, agent, and status. The detail pane
marks the active tmux pane with `*`.
The TUI automatically refreshes indexed workspace state every 30 seconds, so
pane commands and Git state can update without pressing `r`.

## Files

Config:

```text
~/.config/ws/config.yaml
```

Local index:

```text
~/.local/share/ws/workspaces.db
```

## Code Structure

```text
src/main.rs      CLI routing
src/model.rs     shared data types
src/config.rs    config paths and defaults
src/db.rs        SQLite schema and queries
src/remote.rs    SSH and tmux integration
src/commands.rs  command implementations
src/tui.rs       terminal UI
src/util.rs      shell/editor helpers
```

## MVP Scope

Implemented:

- initialize config
- scan remote tmux sessions
- store workspace and pane snapshots
- list workspaces
- attach by workspace ID
- notes and status
- aliases and tags
- Git snapshot per workspace
- rescan preserves notes, aliases, tags, status, and attach history
- basic TUI with search
- edit notes from the TUI
- attach history and recent-first sorting
- duplicate session handling across servers
- attach preflight checks
- recreate missing sessions from indexed paths
- server and session health checks
- config editing from CLI
- archive/unarchive from the TUI
- rescan from the TUI
- scriptable list filters and JSON output

Next:

- pane layout restore
- `.ws.md` note sync
- richer workspace todos
- installable binary release
