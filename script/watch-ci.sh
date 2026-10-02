#!/usr/bin/env bash
# Prints one line per finished CI job for a run, then "RUN <conclusion> <id>" and exits.
# Usage: script/watch-ci.sh [run-id]   (defaults to the newest run on main)
# Uses gh's built-in --jq, so no system jq is needed.
set -uo pipefail
REPO="AadiJo/t3UI"
RUN="${1:-$(gh run list -R "$REPO" --branch main --limit 1 --json databaseId --jq '.[0].databaseId')}"
echo "watching run $RUN"
prev=""
while true; do
  cur=$(gh run view "$RUN" -R "$REPO" --json jobs \
    --jq '.jobs[] | select(.status=="completed") | "\(.name): \(.conclusion)"' 2>/dev/null | sort)
  comm -13 <(echo "$prev") <(echo "$cur")
  prev=$cur
  state=$(gh run view "$RUN" -R "$REPO" --json status,conclusion --jq '"\(.status) \(.conclusion)"' 2>/dev/null)
  if [ "${state%% *}" = "completed" ]; then
    echo "RUN ${state#* } $RUN"
    break
  fi
  sleep 30
done
