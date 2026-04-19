#!/usr/bin/env bash
set -euo pipefail

repo_root="$(pwd)"

printf 'demo/mock-opencode\n'
printf 'Applying changes in %s\n' "$repo_root"
sleep 0.8

cat >"$repo_root/src/lib.rs" <<'EOF'
pub fn greeting(name: &str) -> String {
    format!("Hello, {name}. Review before you commit.")
}

pub fn headline() -> String {
    "Review queue ready.".to_string()
}

pub fn summary(items: &[&str]) -> String {
    items.join(", ")
}

pub fn footer() -> &'static str {
    "Accepted changes only."
}
EOF

cat >"$repo_root/src/review_queue.rs" <<'EOF'
pub fn pending_summary(total: usize, accepted: usize) -> String {
    let remaining = total.saturating_sub(accepted);
    format!("{remaining} change(s) still need review")
}
EOF

printf 'Changes ready. Press Ctrl+C to return to better-review.\n'
sleep 4
