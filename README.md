# Tmux Workbench

[![CI](https://github.com/LeON-Nie-code/tmux-workbench/actions/workflows/ci.yml/badge.svg)](https://github.com/LeON-Nie-code/tmux-workbench/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](Cargo.toml)
[![Release](https://img.shields.io/github/v/release/LeON-Nie-code/tmux-workbench?include_prereleases)](https://github.com/LeON-Nie-code/tmux-workbench/releases)

English | [简体中文](README.zh-CN.md)

Tmux Workbench is a terminal workspace memory manager for local and remote tmux
sessions.

It indexes tmux sessions across your machine and SSH servers, remembers the
project context around them, and gives you one fast CLI/TUI entry point to get
back to work.

```bash
ws
```

<p align="center">
  <img src="docs/assets/demo.gif" alt="Tmux Workbench CLI and TUI demo" width="100%">
</p>

## Why

SSH plus tmux is resilient, but it does not remember enough when your work is
spread across many machines and many projects. Tmux Workbench adds a local
memory layer above tmux:

- server and connection information
- tmux session and pane snapshot
- project path and active command
- git branch, commit, dirty state, ahead/behind counts, and remote URL
- notes, aliases, tags, archive status, and attach history

It does not replace tmux. It makes tmux workspaces easier to find, inspect, and
resume.

## Features

- Index local tmux sessions and remote tmux sessions over SSH.
- Attach back to a workspace by stable ID: `<server>/<tmux-session>`.
- Manage servers from the CLI.
- Browse workspaces in a TUI with search, server filtering, and view modes.
- Preserve notes, aliases, tags, status, and attach history across scans.
- Detect missing tmux sessions without overwriting archive state.
- Capture git repository state for each workspace.
- Refresh in the background without blocking the TUI.
- Store state locally in SQLite.

## Installation

Requirements:

- tmux
- git
- ssh for remote servers

### Install Script

```bash
curl -fsSL https://raw.githubusercontent.com/LeON-Nie-code/tmux-workbench/master/install.sh | bash
```

The script installs `ws` into `~/.local/bin` by default. Set
`TMUX_WORKBENCH_INSTALL_DIR` to override the install directory.

### Homebrew

After tapping the repository:

```bash
brew tap LeON-Nie-code/tmux-workbench
brew install ws
```

For local testing from a checkout:

```bash
brew install --build-from-source ./Formula/ws.rb
```

### Cargo

```bash
cargo install --git https://github.com/LeON-Nie-code/tmux-workbench ws
```

From a local checkout:

```bash
cargo install --path .
```

### Manual Download

Download a binary from the
[Releases](https://github.com/LeON-Nie-code/tmux-workbench/releases) page and
place it somewhere in your `PATH`.

Example for macOS Apple Silicon:

```bash
curl -L -o ws https://github.com/LeON-Nie-code/tmux-workbench/releases/download/v0.1.0/ws-macos-aarch64
chmod +x ws
mkdir -p ~/.local/bin
mv ws ~/.local/bin/ws
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
ws recreate prod/api

ws note prod/api "Backend uses uv. Frontend is in ./web."
ws alias prod/api api
ws tags prod/api work backend
ws status prod/api archived

ws doctor
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

## Contributing

Issues and pull requests are welcome after the project becomes public. See
[CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md),
and [SECURITY.md](SECURITY.md).

## License

MIT. See [LICENSE](LICENSE).
