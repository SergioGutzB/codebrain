# CodeBrain — troubleshooting

## `doctor` fails

| Check | Typical fix |
|-------|-------------|
| config path missing | `codebrain init` or pass `--config` |
| DB open error | Ensure parent dir is writable; delete corrupt DB only if you accept reindex |
| schema mismatch | `codebrain doctor --migrate` |
| embedding dimension mismatch | Align `[embeddings].dimension` with stored meta, then reindex |
| source path missing | Fix `path` under `[sources.*]` |

## Empty MCP answers

1. `codebrain status` — confirm `symbol` / `document` counts &gt; 0.
2. Re-run `codebrain index`.
3. Check logs on **stderr** (`RUST_LOG=debug codebrain serve`).
4. Confirm the MCP client uses absolute paths for binary + config.

## Watcher not reindexing

- `index.watch = true` on `serve`, or run `codebrain watch`.
- Events are debounced (`index.debounce_ms`, default 750).
- Only matching extensions are watched (code langs / `.md` for vaults).
- Excludes (`node_modules`, `.obsidian`, …) skip paths by design.

## ADR did not write a note

- Default `adr.write_vault = false` never touches the vault (by design).
- Pass `write_vault: true` in the tool call **or** enable it in config.
- `vault_source` must name an `obsidian_vault` source that exists on disk.

## `promote_mention` errors

- Requires an existing `mentions` edge between the document and symbol.
- Use tokens from `explore_context` / `graph_neighbors` (`document:…`, `symbol:…`).
- With `linker.auto_promote_explains = true`, high-confidence mentions promote at index time.

## HTTP MCP won't start

- Invalid `mcp.bind` → fix address (`127.0.0.1:8765`).
- Non-loopback without `allow_remote` → refused (see SECURITY.md).
- Port in use → change `bind` or stop the other process.

## Performance

- First `fastembed` index downloads the model (lazy).
- Large monorepos: tighten `languages` / `exclude`, index one `--source` at a time.
- See [`BENCH.md`](./BENCH.md) for measuring index throughput.
