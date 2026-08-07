#!/usr/bin/env bash
# Post-implementation gate: validate + light diff hygiene.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=common.sh
source "$ROOT/scripts/agent/common.sh"

agent_header "postcheck"

"$ROOT/scripts/agent/validate.sh"

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "→ scanning working tree for secret-like filenames"
  if git status --porcelain | grep -E '\.env($|\.)|credentials\.json|id_rsa|.*\.pem$' >/dev/null 2>&1; then
    die 1 "refusing: secret-like files present in git status"
  fi

  echo "→ scanning working tree diffs for Co-Authored-By leftovers"
  if { git diff; git diff --cached; } | grep -Ei 'Co-Authored-By:\s*(Cursor|Claude|GPT|OpenAI|Copilot|OpenCode)' >/dev/null 2>&1; then
    die 1 "found AI Co-Authored-By text in diffs — remove it"
  fi
else
  echo "→ git not initialized; skipping diff hygiene"
fi

agent_ok "postcheck passed"
