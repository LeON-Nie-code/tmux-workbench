# Demo

This page tracks demo assets for the first public release.

## Current Assets

- `docs/assets/demo.gif`
- `docs/demo/workbench.tape`
- `scripts/demo-fixture.sh`

The GIF is generated with VHS from the real `ws` binary and a local fixture
database. It does not use mocked screenshots.

The tape unsets `NO_COLOR` before launching `ws`. Keep that line in place:
without it, crossterm correctly disables ANSI colors and the GIF becomes a
flat grayscale recording.

Regenerate it from the repository root:

```bash
cargo build --release
PATH="$PWD/target/release:$PATH" vhs docs/demo/workbench.tape
```

Planned assets:

- 20-30 second asciinema recording
- real terminal screenshots for the TUI list and detail views

Suggested flow:

1. `ws servers`
2. `ws scan`
3. `ws`
4. search with `server:prod git:dirty`
5. edit a note with `n`
6. attach with `Enter`
