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
Set TypingSpeed 5ms
Set PlaybackSpeed 0.92
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful

Hide
Type "./run-demo.sh"
Enter
Wait+Screen /demo\/mock-model \[balanced\]/
Show

Sleep 1000ms
Ctrl+O
Wait+Screen /New Prompt/
Sleep 1s
Type "Refine greeting text, update footer copy, and add a review queue helper."
Sleep 700ms
Enter
Sleep 1200ms
Wait+Screen /Reviewing files/

Sleep 900ms
Enter
Wait+Screen /Inspecting hunks/
Sleep 1000ms
Escape
Wait+Screen /Reviewing files/
Sleep 900ms
Type "y"
Wait+Screen /\[✓\] src\/lib\.rs/
Sleep 900ms
Type "j"
Sleep 1000ms
Type "x"
Wait+Screen /\[x\] src\/review_queue\.rs/
Sleep 850ms
Type "c"
Wait+Screen /Commit prompt active/
Sleep 700ms
Type "feat: commit accepted review changes"
Sleep 500ms
Ctrl+C
Sleep 500ms
