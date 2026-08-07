# Changelog

All notable changes to CodeBrain are documented here. Format inspired by [Keep a Changelog](https://keepachangelog.com/). Versioning follows [SemVer](https://semver.org/).

## [1.1.0] — 2026-08-06

### Added

- Jira connector (`kind = jira`): issues → `document`, env auth, JQL + pagination
- `RESOLVES` linker: issue keys in source files → symbol→ticket edges
- Confluence connector (`kind = confluence`): pages → `document`, CQL, ADF/HTML body
- Notion connector (`kind = notion`): pages → `document`, search + blocks, `NOTION_TOKEN`
- Cross-refs: Confluence/Notion pages citing issue keys → `references` to Jira documents
- Mentions from Confluence/Notion page bodies → code symbols
- MCP `source_kinds` filter on `list_sources` / `search_symbols` / `explore_context` / `semantic_search`
- MCP resource `codebrain://schema` (node/edge legend for agents)
- Search excerpts highlight the matched query with `**…**` (documents + semantic chunks)
- Docs: `docs/JIRA.md`, `docs/CONFLUENCE.md`, `docs/NOTION.md`, `docs/KNOWLEDGE_GRAPH.md`, `docs/BACKLOG.md`

### Changed

- `resolves` / document `references` linkers only report **new** edges (idempotent reindex)

## [1.0.0] — 2026-08-05

GA local release (plan Fase 6).

### Added

- MCP tool `promote_mention` (MENTIONS → EXPLAINS) with optional `linker.auto_promote_explains` at index time
- Streamable HTTP MCP transport (`mcp.transport = "http"`, bind + `allow_remote` guard)
- Docs: installation, security threat model, troubleshooting, schema policy, bench guide
- `scripts/bench-index.sh` micro + synthetic index bench (cold + warm timings)
- GitHub Actions release workflow for tagged builds (macOS + Linux)
- Neighbor traversal includes `explains`
- `doctor` checks for MCP transport/bind and ADR write-back config

### Changed

- Workspace crate version → `1.0.0`
- `codebrain status` tracks `mentions` / `explains` counts
- Mention linker uses a prebuilt `MentionIndex` (token lookup) instead of O(docs×symbols) scans
- Indexer persists multi-file / multi-document Surreal batches; indexes code sources before vaults
- Extract concurrency follows CPU count (separate from `batch_size` persist chunks)

### Performance note

Synthetic cold index **1k Ruby + 2k notes ≈ 57 s** on Apple M5 16 GB (`embeddings.provider=none`). Stretch target &lt;30 s remains open; see `docs/BENCH.md`.
## [0.6.0] — 2026-08-05

Watch + ADR write-back (plan Fase 5).

### Added

- Debounced filesystem watcher + partial reindex queue
- CLI `codebrain watch`; `index.watch` on `serve`
- MCP `add_architectural_decision` + `ABOUT` edges
- Opt-in vault Markdown write-back (`[adr]`)

## [0.5.0] — 2026-08-05

GraphRAG (plan Fase 4): embeddings providers, chunk HNSW, `semantic_search`, doctor dimension check.

## [0.4.0] — earlier

MCP brain (stdio tools, explore/neighbors) — see plan Fases 0–3.
