#!/usr/bin/env bash
# Type-checks the workspace for aarch64-apple-darwin from a Linux host.
# Uses a locally patched gpui-pre-apple (placeholder shaders) outside the repo; check-only.
set -euo pipefail
PATCH="${T3UI_APPLE_PATCH:-$HOME/L-Projects/t3UI-refs/patches/gpui-pre-apple}"
exec cargo check --target aarch64-apple-darwin \
  --config "patch.crates-io.gpui-pre-apple.path=\"$PATCH\"" "$@"
