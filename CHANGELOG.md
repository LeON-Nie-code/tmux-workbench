# Changelog

All notable changes to Tmux Workbench will be documented in this file.

The project is currently pre-1.0. Breaking changes may happen while the CLI and
configuration format settle.

## Unreleased

## v0.1.1 - 2026-07-10

- Add AI agent context indexing for files such as `AGENTS.md`, `CLAUDE.md`,
  `GEMINI.md`, `.cursorrules`, and `.windsurfrules`.
- Add `ws agent <workspace>` for inspecting indexed agent context.
- Show agent docs in the TUI workspace detail view.
- Add a TUI loading state while preparing tmux attach.
- Update install instructions and default install script version for v0.1.1.

## v0.1.0 - 2026-07-01

- Add local and remote tmux workspace indexing.
- Add the `ws` CLI and TUI.
- Add server management commands.
- Add notes, aliases, tags, archive status, and attach history.
- Add git snapshots for branch, commit, dirty state, ahead/behind, and remote.
- Add concurrent scan with command timeouts.
- Add TUI scan status, server filtering, structured search, and view modes.
