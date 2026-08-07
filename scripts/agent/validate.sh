#!/usr/bin/env bash
# Full quality gate: fmt + clippy + tests.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=common.sh
source "$ROOT/scripts/agent/common.sh"

agent_header "validate"

need_cmd cargo 3

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "→ cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "→ cargo clippy --workspace --all-targets (deny warnings + panic/unwrap family)"
cargo clippy --workspace --all-targets -- \
  -D warnings \
  -D clippy::unwrap_used \
  -D clippy::expect_used \
  -D clippy::panic \
  -D clippy::todo \
  -D clippy::unimplemented \
  -D clippy::unreachable

echo "→ cargo test --workspace"
cargo test --workspace

agent_ok "validate passed"
