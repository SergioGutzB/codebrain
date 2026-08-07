#!/usr/bin/env bash
# Claude Code PreToolUse: block git commit --no-verify / --no-gpg-sign.
set -euo pipefail

if ! command -v python3 >/dev/null 2>&1; then
  exit 0
fi

python3 - <<'PY'
import json, re, sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)

command = ""
tool_input = data.get("tool_input") or data.get("input") or {}
if isinstance(tool_input, dict):
    command = tool_input.get("command") or ""
elif isinstance(data.get("command"), str):
    command = data["command"]

if re.search(r"git\s+commit", command) and re.search(r"(--no-verify|--no-gpg-sign)", command):
    print("Blocked: do not use --no-verify/--no-gpg-sign. No AI Co-Authored-By trailers.", file=sys.stderr)
    sys.exit(2)

sys.exit(0)
PY
