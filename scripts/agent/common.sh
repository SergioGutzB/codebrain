#!/usr/bin/env bash
# Shared helpers for agent scripts.
set -euo pipefail

die() {
  local code="$1"
  shift
  echo "error: $*" >&2
  exit "$code"
}

need_cmd() {
  local cmd="$1"
  local code="${2:-3}"
  command -v "$cmd" >/dev/null 2>&1 || die "$code" "missing required command: $cmd"
}

agent_header() {
  echo "==> codebrain agent: $*"
}

agent_ok() {
  echo "OK: $*"
}
