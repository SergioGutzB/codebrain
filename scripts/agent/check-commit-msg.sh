#!/usr/bin/env bash
# Validate a commit message: conventional type + no AI co-author trailers.
# Usage:
#   ./scripts/agent/check-commit-msg.sh .git/COMMIT_EDITMSG
#   echo "feat(db): msg" | ./scripts/agent/check-commit-msg.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=common.sh
source "$ROOT/scripts/agent/common.sh"

MSG_FILE="${1:-}"
if [[ -n "$MSG_FILE" ]]; then
  MSG="$(cat "$MSG_FILE")"
else
  MSG="$(cat)"
fi

# Drop comment lines (git commit-msg template)
MSG="$(printf '%s\n' "$MSG" | grep -v '^#' || true)"
SUBJECT="$(printf '%s\n' "$MSG" | head -n 1 | tr -d '\r')"

if [[ -z "${SUBJECT// }" ]]; then
  echo "commit-msg: empty subject" >&2
  exit 2
fi

# Conventional Commits: type(scope)?: summary
if ! printf '%s\n' "$SUBJECT" | grep -Eq '^(feat|fix|refactor|test|docs|chore|perf|build|ci|revert)(\([a-z0-9_-]+\))?!?: .+'; then
  echo "commit-msg: subject must match Conventional Commits" >&2
  echo "  got: $SUBJECT" >&2
  echo "  e.g. feat(db): add schema migrate" >&2
  exit 2
fi

if printf '%s\n' "$MSG" | grep -Ei '^Co-Authored-By:' >/dev/null; then
  echo "commit-msg: Co-Authored-By trailers are forbidden" >&2
  exit 2
fi

if printf '%s\n' "$MSG" | grep -Ei '^(Generated-By|Assisted-By|Signed-off-by):.*(cursor|claude|gpt|openai|copilot|opencode|anthropic|gemini)' >/dev/null; then
  echo "commit-msg: AI attribution trailers are forbidden" >&2
  exit 2
fi

# Soft length check on subject
LEN="${#SUBJECT}"
if (( LEN > 72 )); then
  echo "commit-msg: subject is ${LEN} chars (prefer ≤ 72)" >&2
  exit 2
fi

agent_ok "commit-msg ok"
