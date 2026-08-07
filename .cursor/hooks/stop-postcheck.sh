#!/usr/bin/env bash
# After agent stop: nudge postcheck when Rust/schema/config changed.
set -euo pipefail
cat >/dev/null

if ! command -v python3 >/dev/null 2>&1; then
  echo '{}'
  exit 0
fi

dirty=0
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  if git status --porcelain 2>/dev/null | grep -E '\.(rs|surql|toml)$|schemas/' >/dev/null 2>&1; then
    dirty=1
  fi
fi

python3 -c "import json; print(json.dumps({'followup_message': 'Rust/schema/config files changed. Run ./scripts/agent/postcheck.sh (or just post) before claiming done. Never commit with Co-Authored-By.'} if $dirty else {}))"
