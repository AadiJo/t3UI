#!/usr/bin/env bash
# Type-checks (or lints) the workspace for aarch64-apple-darwin from a Linux host.
# Uses a locally patched gpui-pre-apple (placeholder shaders) outside the repo; check-only.
# Usage: script/check-macos.sh [cargo args...]          e.g. -p t3-app
#        CARGO_SUBCOMMAND=clippy script/check-macos.sh --workspace --all-targets -- -D warnings
set -euo pipefail
PATCH="${T3UI_APPLE_PATCH:-$HOME/L-Projects/t3UI-refs/patches/gpui-pre-apple}"
exec cargo "${CARGO_SUBCOMMAND:-check}" --target aarch64-apple-darwin \
  --config "patch.crates-io.gpui-pre-apple.path=\"$PATCH\"" "$@"
