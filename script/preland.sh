#!/usr/bin/env bash
# Run before every push to main. Mirrors CI as closely as this Linux host allows:
# rustfmt, Linux clippy + tests for the GPUI-free crates, and macOS clippy for the whole
# workspace (via the patched cross-check). GPUI crates can't build for Linux here until the
# system libs are installed, so Linux-only cfg paths are only covered by CI.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check

GPUI_FREE=()
for crate in t3-protocol t3-client t3-highlight t3-logic; do
  [ -d "crates/$crate" ] && GPUI_FREE+=(-p "$crate")
done
cargo clippy "${GPUI_FREE[@]}" --all-targets -- -D warnings
cargo test -q "${GPUI_FREE[@]}"
# Crates whose pure-logic modules build without GPUI when default features are off.
for crate in t3-terminal; do
  [ -d "crates/$crate" ] && cargo test -q -p "$crate" --no-default-features
done

CARGO_SUBCOMMAND=clippy script/check-macos.sh --workspace --all-targets -- -D warnings
echo "preland: ok"
