Output docs/assets/demo.gif
Set Shell "bash"
Set FontFamily "JetBrainsMono Nerd Font"
Set FontSize 16
Set LetterSpacing 0
Set LineHeight 1.15
Set Width 1600
Set Height 900
Set Theme "TokyoNight"
Set TypingSpeed 25ms

Hide
Type "unset NO_COLOR"
Enter
Type "export TERM=xterm-256color"
Enter
Type "export COLORTERM=truecolor"
Enter
Type "export HOME=/tmp/tmux-workbench-vhs-demo"
Enter
Type "rm -rf /tmp/tmux-workbench-vhs-demo && ./scripts/demo-fixture.sh /tmp/tmux-workbench-vhs-demo >/dev/null"
Enter
Type "clear"
Enter
Show

Sleep 500ms
Type "ws"
Enter
Sleep 1800ms
Type "/"
Sleep 200ms
Type "server:prod git:dirty"
Sleep 1800ms
Escape
Sleep 400ms
Type "s"
Sleep 800ms
Type "v"
Sleep 800ms
Type "j"
Sleep 800ms
Type "k"
Sleep 800ms
Type "q"
Sleep 500ms
