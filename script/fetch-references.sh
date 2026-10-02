#!/usr/bin/env bash
# Downloads the fork's reference screenshots (docs/reference/*.png) from the
# "reference-assets" GitHub Release. They are too large for git history.
# Regenerate with: e2e/run-local.sh up --server fork --capture   (needs the private fork)
set -euo pipefail
cd "$(dirname "$0")/.."
gh release download reference-assets -R AadiJo/t3UI -p 'reference-png.tar' -D /tmp --clobber
tar -xf /tmp/reference-png.tar -C docs/reference
ls docs/reference/*.png | wc -l
