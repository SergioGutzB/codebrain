#!/usr/bin/env bash
# Inject agent-contract reminder at session start.
set -euo pipefail
# Read stdin (required); ignore contents for now.
cat >/dev/null

python3 - <<'PY'
import json
print(json.dumps({
  "additional_context": (
    "CodeBrain agent contract active. "
    "Before non-trivial coding run ./scripts/agent/preflight.sh. "
    "Before claiming done run ./scripts/agent/postcheck.sh. "
    "Never add Co-Authored-By or AI commit trailers. "
    "Canonical doc: AGENTS.md"
  )
}))
PY
