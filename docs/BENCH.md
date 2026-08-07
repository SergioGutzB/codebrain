# CodeBrain — index bench

Goal (GA stretch): **~1k code files + ~2k notes indexed in &lt; 30 s** on a documented reference machine.

Cold first-index on SurrealKV + tree-sitter is currently **~57 s** on the reference laptop below; warm reindex (hash skip) is near-instant. Treat &lt;30 s as an ongoing performance target, not a gate that blocks v1.0.

## Quick micro-bench (CI / laptop)

Times the fixture corpus (small, deterministic):

```bash
./scripts/bench-index.sh
# or: just bench
```

Writes `bench_index_ms=…` and `bench_warm_index_ms=…` to stderr.

## Synthetic GA bench

```bash
./scripts/bench-index.sh --synthetic \
  --files 1000 \
  --notes 2000 \
  --out /tmp/codebrain-bench
# or: just bench-ga
```

Uses `embeddings.provider = none` so the number reflects graph ingest (discover → extract → persist), not model download.

### Reference result (recorded)

```text
Machine: Apple M5, 16 GB RAM, macOS 26.6
Commit:  v1.0.0 workspace (post batch-persist + linker index)
Command: CODEBRAIN_BIN=./target/release/codebrain \
         ./scripts/bench-index.sh --synthetic --files 1000 --notes 2000
Result:  cold ≈ 57 s (code≈28 s + notes≈16 s + overhead)
         warm reindex ≈ <1–2 s (indexed=0 via BLAKE3)
Pass <30s cold: stretch / not yet met
```

Breakdown on the same machine:

| Corpus | Wall (approx) |
|--------|----------------|
| 1k Ruby files only | ~28.5 s |
| 2k Markdown notes only | ~15.8 s |
| Combined cold | ~57 s |

### Tuning knobs

- `[index].batch_size` — persist chunk size (default 64; bench uses 128)
- Extract concurrency follows CPU count (clamped 4–32), independent of `batch_size`
- Mention linking uses a prebuilt `MentionIndex` (O(tokens) per note)

## Embeddings dogfood (`fastembed`)

Local ONNX MiniLM (`all-MiniLM-L6-v2`, dim 384). First run downloads the model into the fastembed cache
(`FASTEMBED_CACHE_DIR` or `.fastembed_cache`; `HF_HOME` also accepted by `hf-hub`).

```toml
[embeddings]
provider = "fastembed"
model = "all-MiniLM-L6-v2"
dimension = 384
```

After switching from `none`, run a full pass (content hashes alone would skip every file):

```bash
# Prefetch model if Hub downloads flake from the Rust client:
#   python3 -c "from huggingface_hub import snapshot_download; snapshot_download('Qdrant/all-MiniLM-L6-v2-onnx', cache_dir='$HOME/.cache/fastembed')"
export HF_HOME="${HF_HOME:-$HOME/.cache/fastembed}"
export FASTEMBED_CACHE_DIR="${FASTEMBED_CACHE_DIR:-$HOME/.cache/fastembed}"

codebrain index --force   # or plain `index` when chunk table is empty (auto-force)
codebrain doctor          # embeddings.dimension must be ok
codebrain status          # chunk count > 0
```

If an older SurrealKV store errors on chunk/index ops after enabling vectors, point
`database.path` at a fresh directory and re-index (graph-only DBs are cheap to rebuild).

### Reference result (recorded)

```text
Machine: Apple M5, 16 GB RAM, macOS 26.x
Commit:  post index --force + chunk persist fix (workspace 1.1.x)
Corpus:  product-definition-service (~975 Ruby files, 1794 symbols)
         + Obsidian vault (28 notes)
Command: embeddings.provider=fastembed, batch_size=32, model cached
Result:  cold ≈ 116 s → 2296 chunks (code 1794 + notes 502)
         warm reindex ≈ 1.8 s (indexed=0)
Daily DB (same machine, + Jira 19 issues + Confluence 50 pages):
         sequential index ≈ 56 s code + 59 s notes + 5 s jira + 44 s wiki
         → 2668 chunks, resolves=26, mentions≈148
```

## When to re-record

After extractor, Surreal persist, or linker changes that could move cold throughput by &gt;10%.
After embedding provider / chunking changes that move vector build time by &gt;10%.
