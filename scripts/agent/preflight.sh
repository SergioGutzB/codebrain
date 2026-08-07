#!/usr/bin/env bash
# Pre-implementation gate: toolchain + workspace check.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=common.sh
source "$ROOT/scripts/agent/common.sh"

agent_header "preflight"

need_cmd cargo 3
need_cmd rustc 3

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "rustc: $(rustc --version)"
echo "cargo: $(cargo --version)"

echo "→ cargo check --workspace"
cargo check --workspace

echo "→ schema file present"
[[ -f schemas/v1.surql ]] || die 1 "missing schemas/v1.surql"

echo "→ AGENTS.md present"
[[ -f AGENTS.md ]] || die 1 "missing AGENTS.md"

agent_ok "preflight passed"
