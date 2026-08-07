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

## When to re-record

After extractor, Surreal persist, or linker changes that could move cold throughput by &gt;10%.
