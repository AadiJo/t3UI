#!/usr/bin/env bash
# Prints one line per finished CI job for a run, then "RUN <conclusion>" and exits.
# Usage: script/watch-ci.sh [run-id]   (defaults to the newest run on main)
set -uo pipefail
REPO="AadiJo/t3UI"
RUN="${1:-$(gh run list -R "$REPO" --branch main --limit 1 --json databaseId --jq '.[0].databaseId')}"
echo "watching run $RUN"
prev=""
while true; do
  json=$(gh run view "$RUN" -R "$REPO" --json jobs,status,conclusion 2>/dev/null || echo '{}')
  cur=$(jq -r '.jobs[]? | select(.status=="completed") | "\(.name): \(.conclusion)"' <<<"$json" | sort)
  comm -13 <(echo "$prev") <(echo "$cur")
  prev=$cur
  if [ "$(jq -r '.status // ""' <<<"$json")" = "completed" ]; then
    echo "RUN $(jq -r '.conclusion' <<<"$json") $RUN"
    break
  fi
  sleep 30
done
