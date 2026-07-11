# Architecture

Tmux Workbench is a local index for tmux workspaces. It does not replace tmux,
ssh, git, or a terminal emulator. It asks those tools for state, stores a compact
snapshot in SQLite, and uses the original tools when attaching back.

## Data Flow

```text
local/remote tmux
    |
    | tmux list-panes
    v
ws scan
    |
    | git status / agent docs
    v
SQLite index
    |
    +--> ws list / ws stats / ws doctor
    +--> TUI search and detail view
    +--> ws attach -> ssh/tmux attach
```

## Components

- `src/main.rs`: CLI command definitions and dispatch.
- `src/commands.rs`: command-level behavior such as scan, attach, doctor, and
  metadata updates.
- `src/remote.rs`: local and remote command execution, tmux parsing, git
  snapshots, and agent context scanning.
- `src/db.rs`: SQLite schema, migrations, workspace persistence, and user
  metadata preservation.
- `src/tui.rs`: ratatui interface, search/filtering, note editing, archive
  toggles, refresh, and attach loading.
- `src/config.rs`: user config loading and server management.
- `src/model.rs`: shared data structures.

## Persistence

Tmux Workbench stores data locally:

```text
~/.config/ws/config.yaml
~/.local/share/ws/workspaces.db
```

The database keeps two kinds of state:

- discovered state from tmux/git/agent files, refreshed by `ws scan`
- user state such as notes, aliases, tags, archive status, and attach history

Scans preserve user state. If a remote tmux session disappears, the workspace is
marked as missing through `presence` instead of being deleted or automatically
archived.

## Remote Execution

Remote machines are configured as shell commands:

```yaml
servers:
  - name: prod
    ssh: ssh prod
    term: xterm-256color
    local: false
```

Because `ssh` is stored as a command, users can rely on `~/.ssh/config`,
non-default ports, ProxyCommand, cloud-generated host aliases, and jump hosts.

For example:

```bash
ws add-server prod --ssh "ssh -p 2222 user@example.com"
ws add-server via-bastion --ssh "ssh prod-via-bastion"
```

## Attach

Attach is intentionally not reimplemented. Tmux Workbench resolves a workspace,
verifies that the tmux session exists, records attach history, restores terminal
state, and hands off to:

```text
ssh -t <host> 'TERM=xterm-256color tmux attach -t <session>'
```

For local workspaces it runs the tmux attach command directly.

## Agent Context

During scan, Tmux Workbench checks the workspace root for common agent
instruction files:

- `AGENTS.md`
- `AGENT.md`
- `CLAUDE.md`
- `GEMINI.md`
- `.cursorrules`
- `.windsurfrules`

Only a small preview is stored. This makes the TUI useful for recognizing what a
workspace is for without turning the database into a document store.

## Privacy

Tmux Workbench does not send telemetry. `ws stats` reads only the local SQLite
index and summarizes local usage.
