#!/usr/bin/env bash
# Install repo-local git hooks (Conventional Commits + no AI co-authors).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Initializing git repository…"
  git init
fi

HOOKS_DIR="$ROOT/.githooks"
mkdir -p "$HOOKS_DIR"

cat > "$HOOKS_DIR/commit-msg" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(git rev-parse --show-toplevel)"
exec "$REPO_ROOT/scripts/agent/check-commit-msg.sh" "$1"
EOF

chmod +x "$HOOKS_DIR/commit-msg"
chmod +x "$ROOT"/scripts/agent/*.sh

# Prefer repo-local hooks over any global core.hooksPath.
git config --local core.hooksPath .githooks

echo "Installed hooks in .githooks/ (core.hooksPath=.githooks)"
echo "Policy: Conventional Commits + no Co-Authored-By / AI trailers"
