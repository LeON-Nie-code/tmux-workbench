# Comparison

Tmux Workbench overlaps with several tmux and terminal tools, but its goal is
different: it is a workspace memory manager for local and remote tmux sessions.

## tmux-resurrect

tmux-resurrect saves and restores tmux sessions, windows, panes, and commands.

Tmux Workbench does not try to restore full tmux layouts. It indexes existing
local and remote tmux sessions, remembers project metadata, and helps you find
and reattach to the right workspace.

Use tmux-resurrect when you want session restoration. Use Tmux Workbench when
you already keep sessions alive and need a searchable memory layer across
machines.

## tmux-continuum

tmux-continuum automates tmux-resurrect snapshots.

Tmux Workbench is not an autosave engine. It scans tmux state, records context,
and keeps notes, tags, archive status, git state, attach history, and agent docs
in a local index.

## Zellij

Zellij is a terminal workspace and multiplexer with layouts, sessions, and a
modern UI.

Tmux Workbench is not a multiplexer. It assumes users already have tmux sessions
running, often on remote machines, and provides a CLI/TUI launcher and memory
layer around them.

## Plain SSH Config

SSH config is the right place for connection details: users, ports, keys, jump
hosts, and aliases.

Tmux Workbench builds on that instead of replacing it. Store connection behavior
in `~/.ssh/config`, then add the server:

```bash
ws add-server prod --ssh "ssh prod"
```

For non-default ports, either use SSH config or pass the full command:

```bash
ws add-server lab --ssh "ssh -p 2222 user@example.com"
```

## Terminal Emulator Workspaces

Terminal emulators can remember tabs or windows on one machine.

Tmux Workbench focuses on tmux sessions as the durable unit, so a workspace can
live on a remote server and be reattached from any terminal that can run `ssh`
and `tmux`.

## Summary

Tmux Workbench is useful when:

- you use SSH and tmux every day
- you keep many long-running tmux sessions
- those sessions are spread across multiple machines
- many sessions run coding agents such as Claude Code, Codex, Gemini, or Aider
- you want searchable project context without replacing tmux
