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
Set TypingSpeed 8ms
Set PlaybackSpeed 0.92
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful

Hide
Type "tmpdir=$(mktemp -d); cp -R demo/fixture/. $tmpdir; git -C $tmpdir init -q; git -C $tmpdir config user.name demo; git -C $tmpdir config user.email demo@example.com; git -C $tmpdir add .; git -C $tmpdir commit -qm init"
Enter
Type "cat > $tmpdir/src/lib.rs <<'EOF'"
Enter
Type "pub fn greeting(name: &str) -> String {"
Enter
Type "    format!(\"Hello, {name}. Review before you commit.\")"
Enter
Type "}"
Enter
Type ""
Enter
Type "pub fn headline() -> String {"
Enter
Type "    \"Review queue ready.\".to_string()"
Enter
Type "}"
Enter
Type ""
Enter
Type "pub fn summary(items: &[&str]) -> String {"
Enter
Type "    items.join(\", \")"
Enter
Type "}"
Enter
Type ""
Enter
Type "pub fn footer() -> &'static str {"
Enter
Type "    \"Accepted changes only.\""
Enter
Type "}"
Enter
Type "EOF"
Enter
Type "cat > $tmpdir/src/review_queue.rs <<'EOF'"
Enter
Type "pub fn pending_summary(total: usize, accepted: usize) -> String {"
Enter
Type "    let remaining = total.saturating_sub(accepted);"
Enter
Type "    format!(\"{remaining} change(s) still need review\")"
Enter
Type "}"
Enter
Type "EOF"
Enter
Type "cd $tmpdir && cargo run --manifest-path /home/ricardo/better-review/Cargo.toml"
Enter
Wait+Screen /No code changes yet/
Show

Sleep 1000ms
Type "r"
Wait+Screen /Refreshed workspace review\. 2 changed file\(s\) ready\./
Sleep 900ms
Enter
Wait+Screen /reviewing files/

Sleep 900ms
Enter
Wait+Screen /inspecting hunks/
Sleep 1000ms
Escape
Wait+Screen /reviewing files/
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
