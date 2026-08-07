#!/usr/bin/env bash
# Micro / synthetic index bench for CodeBrain GA.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SYNTHETIC=0
FILES=1000
NOTES=2000
OUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --synthetic) SYNTHETIC=1; shift ;;
    --files) FILES="$2"; shift 2 ;;
    --notes) NOTES="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    -h|--help)
      sed -n '1,40p' "$ROOT/docs/BENCH.md"
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

BIN="${CODEBRAIN_BIN:-}"
if [[ -z "$BIN" ]]; then
  echo "building release binary (first run can take several minutes)…" >&2
  cargo build -p codebrain-cli --release
  BIN="$ROOT/target/release/codebrain"
fi

if [[ ! -x "$BIN" ]]; then
  echo "codebrain binary not found/executable: $BIN" >&2
  exit 1
fi

echo "using binary: $BIN" >&2

if [[ "$SYNTHETIC" -eq 1 ]]; then
  OUT="${OUT:-$(mktemp -d /tmp/codebrain-bench.XXXXXX)}"
  mkdir -p "$OUT/repo" "$OUT/vault" "$OUT/db"
  echo "generating $FILES code files + $NOTES notes under $OUT" >&2
  python3 - <<PY
from pathlib import Path
out = Path("$OUT")
repo = out / "repo"
vault = out / "vault"
files = int("$FILES")
notes = int("$NOTES")
for i in range(1, files + 1):
    (repo / f"f{i:05d}.rb").write_text(
        f"module M{i:05d}\\n  def call_{i:05d}\\n    1\\n  end\\nend\\n"
    )
for i in range(1, notes + 1):
    (vault / f"Note-{i:05d}.md").write_text(
        f"# Note {i:05d}\\n\\nMentions M{i:05d} occasionally.\\n"
    )
print(f"generated files={files} notes={notes}", flush=True)
PY
  cat >"$OUT/codebrain.toml" <<EOF
[database]
path = "$OUT/db"

[sources.code]
kind = "git_repo"
path = "$OUT/repo"
languages = ["ruby"]

[sources.notes]
kind = "obsidian_vault"
path = "$OUT/vault"

[embeddings]
provider = "none"
model = "all-MiniLM-L6-v2"
dimension = 384

[index]
watch = false
batch_size = 128

[linker]
mention_threshold = 0.99
auto_promote_explains = false

[mcp]
transport = "stdio"
EOF
  CONFIG="$OUT/codebrain.toml"
else
  CONFIG="$ROOT/testdata/codebrain.fixture.toml"
fi

echo "indexing with config=$CONFIG …" >&2
START=$(python3 - <<'PY'
import time
print(time.time())
PY
)

"$BIN" --config "$CONFIG" index | tee /tmp/codebrain-bench-index.out >&2

END=$(python3 - <<'PY'
import time
print(time.time())
PY
)

ELAPSED_MS=$(python3 - <<PY
start=float("$START")
end=float("$END")
print(int((end-start)*1000))
PY
)

echo "bench_index_ms=$ELAPSED_MS config=$CONFIG" >&2

# Warm second pass (content-hash skip) — should be near-instant.
WARM_START=$(python3 - <<'PY'
import time
print(time.time())
PY
)
"$BIN" --config "$CONFIG" index >/tmp/codebrain-bench-index-warm.out
WARM_END=$(python3 - <<'PY'
import time
print(time.time())
PY
)
WARM_MS=$(python3 - <<PY
print(int((float("$WARM_END")-float("$WARM_START"))*1000))
PY
)
echo "bench_warm_index_ms=$WARM_MS" >&2

if [[ "$SYNTHETIC" -eq 1 ]]; then
  SECONDS_F=$(python3 - <<PY
print(round($ELAPSED_MS/1000.0, 2))
PY
)
  echo "synthetic_wall_seconds=$SECONDS_F files=$FILES notes=$NOTES out=$OUT" >&2
fi
