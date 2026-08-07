#!/usr/bin/env bash
# Guard git commit: block --no-verify / --no-gpg-sign; remind no AI co-authors.
set -euo pipefail

if ! command -v python3 >/dev/null 2>&1; then
  echo '{"permission":"allow"}'
  exit 0
fi

python3 - <<'PY'
import json, re, sys

data = json.load(sys.stdin)
command = data.get("command") or ""

if re.search(r"(--no-verify|--no-gpg-sign)", command):
    print(json.dumps({
        "permission": "deny",
        "user_message": "Blocked: do not bypass git hooks (--no-verify / --no-gpg-sign) unless explicitly requested.",
        "agent_message": "Remove --no-verify/--no-gpg-sign. Validate with ./scripts/agent/check-commit-msg.sh. Never add Co-Authored-By.",
    }))
else:
    print(json.dumps({
        "permission": "allow",
        "agent_message": "Conventional Commits only; never add Co-Authored-By or AI attribution trailers.",
    }))
PY
