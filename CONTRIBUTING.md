# Contributing

Thanks for considering a contribution to Tmux Workbench.

## Development

```bash
cargo fmt
cargo test
cargo run -- scan
cargo run
```

Keep changes focused. For TUI behavior, include the user-facing shortcut or
workflow in the pull request description.

## Design Principles

- Keep tmux and SSH as the source of truth.
- Prefer local, inspectable state over hidden cloud state.
- Make slow or broken servers visible without blocking the TUI.
- Preserve user metadata during scans.

## Before Opening a PR

- Run `cargo fmt`.
- Run `cargo test`.
- Avoid committing local config, database files, or product planning notes.
