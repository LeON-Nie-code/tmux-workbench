# Tmux Workbench

[![CI](https://github.com/LeON-Nie-code/tmux-workbench/actions/workflows/ci.yml/badge.svg)](https://github.com/LeON-Nie-code/tmux-workbench/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](Cargo.toml)
[![Release](https://img.shields.io/github/v/release/LeON-Nie-code/tmux-workbench?include_prereleases)](https://github.com/LeON-Nie-code/tmux-workbench/releases)

English | [简体中文](README.zh-CN.md)

Tmux Workbench is a workspace memory manager for developers running SSH, tmux,
and AI coding agents across multiple machines.

It indexes local and remote tmux sessions, remembers the project context around
them, and gives you one fast CLI/TUI entry point to get back to work.

```bash
ws
```

<p align="center">
  <img src="docs/assets/demo.gif" alt="Tmux Workbench CLI and TUI demo" width="100%">
</p>

## Why

SSH plus tmux is resilient, but it does not remember enough when your work is
spread across many machines, many projects, and long-running coding-agent
sessions. Tmux Workbench adds a local memory layer above tmux:

- server and connection information
- tmux session and pane snapshot
- project path and active command
- git branch, commit, dirty state, ahead/behind counts, and remote URL
- AI agent instruction files such as `AGENTS.md` and `CLAUDE.md`
- notes, aliases, tags, archive status, and attach history

It does not replace tmux. It makes tmux workspaces easier to find, inspect, and
resume.

## AI Agent Workflows

Many workspaces now have a long-running coding agent pane such as Claude Code,
Codex, Gemini, or Aider. Tmux Workbench treats those panes as first-class
workspace context:

- prefer agent panes when choosing the workspace root
- index agent instruction files like `CLAUDE.md`, `AGENTS.md`, and
  `.cursorrules`
- show agent docs in the TUI detail view
- expose indexed context with `ws agent <workspace>`
- keep attach loading visible while remote tmux checks run

## Features

- Index local tmux sessions and remote tmux sessions over SSH.
- Attach back to a workspace by stable ID: `<server>/<tmux-session>`.
- Manage servers from the CLI.
- Browse workspaces in a TUI with search, server filtering, and view modes.
- Preserve notes, aliases, tags, status, and attach history across scans.
- Detect missing tmux sessions without overwriting archive state.
- Capture git repository state for each workspace.
- Detect AI agent context files in each workspace root.
- Refresh in the background without blocking the TUI.
- Diagnose local and remote setup with `ws doctor`.
- Show local usage stats with `ws stats`.
- Store state locally in SQLite.

## Installation

Requirements:

- tmux
- git
- ssh for remote servers

### Recommended

```bash
curl -fsSL https://raw.githubusercontent.com/LeON-Nie-code/tmux-workbench/master/install.sh | bash
```

The installer downloads the right binary for your platform, installs `ws` into
a writable directory, verifies the install, and prints a PATH fix if needed.

To choose a custom install directory:

```bash
curl -fsSL https://raw.githubusercontent.com/LeON-Nie-code/tmux-workbench/master/install.sh \
  | TMUX_WORKBENCH_INSTALL_DIR="$HOME/bin" bash
```

### Other Methods

Cargo from GitHub:

```bash
cargo install --git https://github.com/LeON-Nie-code/tmux-workbench ws
```

Cargo from a local checkout:

```bash
git clone https://github.com/LeON-Nie-code/tmux-workbench.git
cd tmux-workbench
cargo install --path .
```

Manual download:

```bash
curl -L -o ws https://github.com/LeON-Nie-code/tmux-workbench/releases/download/v0.1.2/ws-macos-aarch64
chmod +x ws
mkdir -p ~/.local/bin
mv ws ~/.local/bin/ws
```

Homebrew:

Homebrew support is available from this repository tap. Homebrew 6 requires
trusting custom taps before loading their formulae:

```bash
brew tap LeON-Nie-code/tmux-workbench https://github.com/LeON-Nie-code/tmux-workbench
brew trust LeON-Nie-code/tmux-workbench
brew install LeON-Nie-code/tmux-workbench/ws
```

Verify:

```bash
ws --version
ws doctor
```

## Quick Start

```bash
ws init
ws servers
ws add-server prod --ssh "ssh prod"
ws scan
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
ws add-server local-dev --local
ws remove-server prod

ws scan
ws list
ws list --server prod
ws list --status active
ws list --all
ws list --json

ws attach prod/api
ws agent prod/api
ws recreate prod/api

ws note prod/api "Backend uses uv. Frontend is in ./web."
ws alias prod/api api
ws tags prod/api work backend
ws status prod/api archived

ws doctor
ws stats
ws open-config
```

Remote server commands use your system `ssh`, so existing `~/.ssh/config`,
keys, ProxyCommand, and generated cloud SSH hosts continue to work.

## TUI

```bash
ws
```

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

The demo GIF is recorded with [VHS](https://github.com/charmbracelet/vhs) from
the real `ws` binary against a local fixture database. See
[docs/demo/workbench.tape](docs/demo/workbench.tape).

## Configuration

Config file:

```text
~/.config/ws/config.yaml
```

Example:

```yaml
servers:
  - name: local
    ssh: ""
    term: xterm-256color
    local: true
  - name: prod
    ssh: ssh prod
    term: xterm-256color
    local: false
```

Local index:

```text
~/.local/share/ws/workspaces.db
```

## First Run

If `ws` has not indexed anything yet, it prints the shortest useful setup path:

```bash
ws scan
ws
```

For remote machines:

```bash
ws add-server prod --ssh "ssh prod"
ws scan
```

If something does not connect, run:

```bash
ws doctor
```

`ws doctor` checks local commands, config/database paths, SSH reachability, tmux
availability, and whether indexed workspaces still exist on their tmux server.

`ws stats` is local-only. It summarizes indexed workspaces, attach counts,
missing sessions, and the most-used servers without sending any telemetry.

## Architecture

Tmux Workbench reads tmux state, stores a local index, and uses tmux/ssh for
attach and discovery.

```text
tmux list-panes  ->  ws scan  ->  SQLite index  ->  CLI/TUI
       git status  ->  git snapshot /
```

Stack:

- Rust
- clap for CLI parsing
- ratatui + crossterm for TUI
- rusqlite for the local index
- system `ssh`, `tmux`, and `git`

## Status

Tmux Workbench is pre-1.0 and currently optimized for a real SSH + tmux daily
workflow. The CLI and database format may still change.

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
- asciinema or GIF demo
- more release targets
- dedicated Homebrew tap repository when the project is public

See [ROADMAP.md](ROADMAP.md) for the current project direction.
See [docs/architecture.md](docs/architecture.md) for internals and
[docs/comparison.md](docs/comparison.md) for how Tmux Workbench differs from
tmux-resurrect, tmux-continuum, Zellij, and plain SSH config.

## Contributing

Issues and pull requests are welcome after the project becomes public. See
[CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md),
and [SECURITY.md](SECURITY.md).

## License

MIT. See [LICENSE](LICENSE).
