Output demo/better-review.gif
Output demo/better-review.mp4

Require bash
Require cargo
Require git
Require ffmpeg
Require ttyd

Set Shell "bash"
Set FontSize 18
Set Width 1480
Set Height 920
Set Padding 24
Set Margin 16
Set MarginFill "#11111b"
Set BorderRadius 8
Set CursorBlink false
Set TypingSpeed 35ms
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful

Hide
Type "./demo/run-demo.sh"
Enter
Wait+Screen /demo\/mock-model \[balanced\]/
Show

Sleep 1s
Ctrl+O
Sleep 500ms
Type "Tighten the greeting, update the footer copy, and add a review queue helper."
Sleep 400ms
Enter
Wait+Screen /Reviewing files/

Sleep 1s
Enter
Wait+Screen /Inspecting hunks/
Sleep 500ms
Escape
Wait+Screen /Reviewing files/
Sleep 500ms
Type "y"
Sleep 800ms
Type "j"
Sleep 500ms
Type "x"
Sleep 900ms
Type "c"
Wait+Screen /Commit prompt active/
Sleep 500ms
Type "feat: commit accepted review changes"
Sleep 300ms
Enter
Sleep 2s
Ctrl+C
Sleep 500ms
