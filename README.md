# Tmux Workbench

[![CI](https://github.com/LeON-Nie-code/tmux-workbench/actions/workflows/ci.yml/badge.svg)](https://github.com/LeON-Nie-code/tmux-workbench/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](Cargo.toml)
[![Release](https://img.shields.io/github/v/release/LeON-Nie-code/tmux-workbench?include_prereleases)](https://github.com/LeON-Nie-code/tmux-workbench/releases)

**Tmux Workbench** is a terminal workspace memory manager for local and remote
tmux sessions. It indexes your tmux workspaces across local machines and SSH
servers, remembers project context, and gives you one fast CLI/TUI entry point
to return to work.

The project name is **Tmux Workbench**. The command is intentionally short:

```bash
ws
```

中文：**Tmux Workbench** 是一个面向本机和远程 tmux session 的终端工作区记忆工具。
它会索引你在不同服务器上的 tmux workspace，记录项目路径、pane、git 状态、备注、
标签和最近进入记录，让你用一个 `ws` 命令回到完整工作现场。

## Why

SSH plus tmux is resilient, but it is not enough when you have many projects
spread across many servers. Tmux Workbench adds a local memory layer on top of
tmux:

- where a workspace lives
- how to reconnect to it
- which tmux session and panes are active
- which project path the agent or shell is using
- git branch, commit, dirty state, ahead/behind counts, and remote URL
- notes, aliases, tags, archive status, and attach history

中文：它不是 tmux 的替代品，而是 tmux + SSH 工作流上方的一层“项目记忆”。

## Features

- Index local tmux sessions and remote tmux sessions over SSH.
- Attach back to a workspace by stable ID: `<server>/<tmux-session>`.
- Manage servers from the CLI: `ws servers`, `ws add-server`, `ws remove-server`.
- Use a TUI with search, server filtering, active/archived views, and notes.
- Preserve user metadata across scans.
- Detect missing tmux sessions without overwriting archive status.
- Capture git branch, short commit, dirty state, ahead/behind counts, and remote URL.
- Refresh in the background without blocking the TUI.
- Store everything locally in SQLite.

中文功能概览：

- 支持本机 tmux 和 SSH 服务器上的 tmux。
- 稳定 workspace ID：`<server>/<tmux-session>`。
- 支持备注、别名、标签、归档、最近进入次数。
- 支持 git 信息和远程仓库链接展示。
- TUI 支持搜索、server 过滤、active/archived 视图。

## Install

### From Source

Requirements:

- Rust 1.85 or newer
- tmux
- git
- ssh, if you use remote servers

Install the `ws` command from this repository:

```bash
cargo install --path .
```

For development:

```bash
cargo run -- <command>
```

### From GitHub Releases

Pre-release binaries are published on the
[Releases](https://github.com/LeON-Nie-code/tmux-workbench/releases) page.

For macOS Apple Silicon:

```bash
curl -L -o ws https://github.com/LeON-Nie-code/tmux-workbench/releases/download/v0.1.0/ws-macos-aarch64
chmod +x ws
mkdir -p ~/.local/bin
mv ws ~/.local/bin/ws
```

Make sure `~/.local/bin` is in your `PATH`.

中文安装：

```bash
cargo install --path .
```

或者从 GitHub Releases 下载对应平台的 `ws` 二进制文件。

## Quick Start

Create the default config:

```bash
ws init
```

List configured servers:

```bash
ws servers
```

Add servers:

```bash
ws add-server prod --ssh "ssh prod"
ws add-server laptop --local
```

Scan tmux sessions:

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

中文快速开始：

```bash
ws init
ws add-server prod --ssh "ssh prod"
ws scan
ws
```

## CLI Reference

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

Run:

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

中文 TUI 快捷键：

```text
Enter  进入 workspace
/      搜索
n      编辑 note
a      归档/取消归档
v      切换 all / active / archived
s      切换 server 过滤
r      重新扫描
q      退出
```

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

Tmux Workbench does not replace tmux. It reads tmux state, stores a local index,
and uses tmux/ssh for attach and discovery.

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

## Project Status

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
- more release binaries
- Homebrew tap after the release flow stabilizes

See [ROADMAP.md](ROADMAP.md) for the current project direction.

## Contributing

Issues and pull requests are welcome after the project becomes public. See
[CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md),
and [SECURITY.md](SECURITY.md).

中文：项目公开后欢迎提交 issue 和 PR。当前阶段仍在打磨产品形态和核心工作流。

## License

MIT. See [LICENSE](LICENSE).
