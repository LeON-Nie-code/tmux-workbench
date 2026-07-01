# ws

`ws` is a remote workspace memory manager for terminal-based development.

It indexes workspaces across SSH servers and tmux sessions, stores the useful
context locally, and gives you one command or TUI to jump back into a project.

## Concept

A workspace is not just a tmux session. It is:

- server
- SSH command
- tmux session
- project path
- running agent or shell
- pane snapshot
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

List indexed workspaces:

```bash
cargo run -- list
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

## Files

Config:

```text
~/.config/ws/config.yaml
```

Local index:

```text
~/.local/share/ws/workspaces.db
```

## MVP Scope

Implemented:

- initialize config
- scan remote tmux sessions
- store workspace and pane snapshots
- list workspaces
- attach by workspace ID
- notes and status
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

Next:

- pane layout restore
- richer workspace notes and todos
- installable binary release
