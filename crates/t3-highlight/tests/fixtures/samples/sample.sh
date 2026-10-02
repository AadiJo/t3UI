#!/usr/bin/env bash
set -euo pipefail

# Build the app and copy the bundle into dist/.
VERSION="${T3UI_VERSION:-0.1.0}"
TARGET_DIR=${CARGO_TARGET_DIR:-target}

if [[ "$(uname -s)" == "Darwin" ]]; then
  cargo build --release -p t3-app
else
  echo "skipping macOS bundle on $(uname -s)" >&2
  exit 0
fi

for file in "$TARGET_DIR"/release/*.dylib; do
  cp -v "$file" dist/ 2>/dev/null || true
done

export PATH="$HOME/.cargo/bin:$PATH"
npx t3@nightly --port 3773 &
echo "started server with pid $! at version $VERSION" | tee -a server.log
