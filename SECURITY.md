# Security Policy

## Supported Versions

Tmux Workbench is pre-1.0. Security fixes are applied to the latest commit on
the default branch until the project starts cutting stable releases.

## Reporting a Vulnerability

Please do not open a public issue for security-sensitive reports.

For now, open a private GitHub security advisory if available, or contact the
maintainer directly through GitHub. Include:

- affected version or commit
- operating system
- reproduction steps
- impact

## Scope

Tmux Workbench shells out to local tools such as `ssh`, `tmux`, and `git`.
Reports involving command construction, config parsing, local database handling,
or unsafe terminal behavior are in scope.
