#!/usr/bin/env bash
set -euo pipefail

command_name="${1:-}"

case "$command_name" in
  models)
    cat <<'EOF'
demo/mock-model
{
  "variants": {
    "balanced": {}
  }
}
EOF
    ;;
  run)
    shift
    repo_path=""

    while (($#)); do
      case "$1" in
        --dir)
          repo_path="${2:-}"
          shift 2
          ;;
        --format|--model|--variant)
          shift 2
          ;;
        *)
          shift
          ;;
      esac
    done

    if [[ -z "$repo_path" ]]; then
      printf 'missing --dir\n' >&2
      exit 1
    fi

    sleep 0.8

    cat >"$repo_path/src/lib.rs" <<'EOF'
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

    cat >"$repo_path/src/review_queue.rs" <<'EOF'
pub fn pending_summary(total: usize, accepted: usize) -> String {
    let remaining = total.saturating_sub(accepted);
    format!("{remaining} change(s) still need review")
}
EOF

    printf '{"status":"ok"}\n'
    ;;
  *)
    printf 'unsupported command: %s\n' "$command_name" >&2
    exit 1
    ;;
esac
