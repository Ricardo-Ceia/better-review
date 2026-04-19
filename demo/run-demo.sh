#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary_path="$root_dir/target/debug/better-review"
cargo build --quiet --manifest-path "$root_dir/Cargo.toml" --bin better-review

demo_root="$(mktemp -d "${TMPDIR:-/tmp}/better-review-demo.XXXXXX")"
cleanup() {
  rm -rf "$demo_root"
}
trap cleanup EXIT

mkdir -p "$demo_root/bin" "$demo_root/repo"
cp -R "$root_dir/demo/fixture/." "$demo_root/repo"
cp "$root_dir/demo/mock-opencode.sh" "$demo_root/bin/opencode"
chmod +x "$demo_root/bin/opencode"

git -C "$demo_root/repo" init -q
git -C "$demo_root/repo" config user.name "better-review demo"
git -C "$demo_root/repo" config user.email "demo@example.com"
git -C "$demo_root/repo" add .
git -C "$demo_root/repo" commit -qm "Initial commit"

cd "$demo_root/repo"
printf 'better-review demo repo\n'
PATH="$demo_root/bin:$PATH" "$binary_path"
