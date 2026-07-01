Output docs/assets/demo.gif
Set Shell "bash"
Set FontFamily "JetBrains Mono"
Set FontSize 18
Set Width 1100
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 25ms

Type "export HOME=/tmp/tmux-workbench-vhs-demo"
Enter
Sleep 300ms
Type "rm -rf /tmp/tmux-workbench-vhs-demo && ./scripts/demo-fixture.sh /tmp/tmux-workbench-vhs-demo >/dev/null"
Enter
Sleep 300ms
Type "clear"
Enter
Sleep 300ms

Type "ws --help"
Enter
Sleep 1800ms
Type "clear"
Enter
Sleep 300ms

Type "ws servers"
Enter
Sleep 1600ms
Type "clear"
Enter
Sleep 300ms

Type "ws list --all"
Enter
Sleep 2200ms
Type "clear"
Enter
Sleep 300ms

Type "ws list --server prod --status active"
Enter
Sleep 2000ms
Type "clear"
Enter
Sleep 300ms

Type "ws list --json"
Enter
Sleep 2200ms
Type "clear"
Enter
Sleep 300ms

Type "ws"
Enter
Sleep 1200ms
Type "/"
Sleep 200ms
Type "server:prod git:dirty"
Sleep 1600ms
Escape
Sleep 400ms
Type "s"
Sleep 600ms
Type "v"
Sleep 600ms
Type "j"
Sleep 500ms
Type "q"
Sleep 500ms
