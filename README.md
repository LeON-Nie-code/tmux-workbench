# Tmux Workbench

**Tmux Workbench** is a terminal workspace memory manager for local and remote
tmux sessions.

It indexes tmux sessions across your machine and SSH servers, stores the useful
context locally, and gives you a fast CLI/TUI entry point to jump back into a
project. The command is intentionally short:

```bash
ws
```

## Why

SSH plus tmux is resilient, but it does not remember enough project context when
you have many projects across many servers. Tmux Workbench adds a local memory
layer:

- which server a workspace lives on
- which tmux session to attach to
- project path
- running panes and active command
- git branch, commit, dirty state, ahead/behind, and remote URL
- notes, tags, aliases, status, and attach history

The stable workspace ID is:

```text
<server>/<tmux-session>
```

## Install

From this repository:

```bash
cargo install --path .
```

During development:

```bash
cargo run -- <command>
```

Binary releases and Homebrew packaging are planned.

## Quick Start

Create the default config:

```bash
ws init
```

List configured servers:

```bash
ws servers
```

Add a server:

```bash
ws add-server prod --ssh "ssh prod"
ws add-server laptop --local
```

Scan workspaces:

```bash
ws scan
```

Open the TUI:

```bash
ws
```

Attach directly:

```bash
ws attach prod/api
```

## CLI

```bash
ws servers
ws add-server prod --ssh "ssh prod"
ws remove-server prod
ws scan
ws list
ws list --server prod
ws list --status active
ws list --all
ws list --json
ws attach prod/api
ws recreate prod/api
ws note prod/api "Backend uses uv. Frontend is in ./web."
ws alias prod/api api
ws tags prod/api work backend
ws doctor
ws open-config
```

Remote server commands use your system `ssh`, so existing `~/.ssh/config`,
keys, ProxyCommand, and generated cloud SSH hosts continue to work.

## TUI

Shortcuts:

```text
Enter  attach
/      search
n      edit note in $EDITOR
a      archive or unarchive
v      cycle all / active / archived
s      cycle server filter
z      jump between archived and all
r      rescan
j/k    move
q      quit
```

Search supports plain text and filters:

```text
server:prod status:active tag:backend git:dirty
```

Git filters include `dirty`, `clean`, `remote`, `ahead`, `behind`, branch text,
commit text, and remote URL text.

## Files

Config:

```text
~/.config/ws/config.yaml
```

Local index:

```text
~/.local/share/ws/workspaces.db
```

## Design

Tmux Workbench does not replace tmux. It reads tmux state, stores a local index,
and uses tmux/ssh for attach and discovery. The project name is Tmux Workbench;
the binary command remains `ws`.

Stack:

- Rust
- clap for CLI parsing
- ratatui + crossterm for TUI
- rusqlite for the local index
- system `ssh`, `tmux`, and `git`

Code structure:

```text
src/main.rs      CLI routing
src/model.rs     shared data types
src/config.rs    config paths and server config
src/db.rs        SQLite schema and queries
src/remote.rs    SSH, tmux, and git integration
src/commands.rs  command implementations
src/tui.rs       terminal UI
src/util.rs      shell/editor helpers
```

## Status

Implemented:

- local and remote tmux indexing
- concurrent scan with command timeouts
- TUI auto-refresh with visible scan status
- server management CLI
- workspace notes, aliases, tags, archive status
- presence tracking for missing tmux sessions
- attach history
- git snapshots
- explicit SQLite `user_version`
- structured list and JSON output

Planned:

- pane layout restore
- demo GIF or asciinema
- packaged binary releases
