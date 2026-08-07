# CodeBrain — installation

## Requirements

- Rust **1.85+** (`rustup`)
- macOS or Linux
- ~200 MB free for the embedded SurrealKV store + optional embedding models

## Build from source

```bash
git clone https://github.com/codebrain/codebrain.git
cd codebrain
cargo build -p codebrain-cli --release
```

Binary: `./target/release/codebrain`.

Optional: copy it onto your `PATH`.

## First-time setup

Camino guiado de un día (recomendado): [`SETUP_DAY.md`](./SETUP_DAY.md).

```bash
# Creates config template + applies schema
./target/release/codebrain init --config ./codebrain.toml

# Edit sources (git_repo / obsidian_vault / jira), then:
./target/release/codebrain doctor --migrate
./target/release/codebrain index
./target/release/codebrain status
```

For Jira / Confluence / Notion, see [`docs/JIRA.md`](./JIRA.md), [`docs/CONFLUENCE.md`](./CONFLUENCE.md), and [`docs/NOTION.md`](./NOTION.md).

Mapa de módulos y cómo se relacionan: [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md). Backlog: [`BACKLOG.md`](./BACKLOG.md).

Smoke fixtures (no personal repos required):

```bash
cargo run -p codebrain-cli -- --config testdata/codebrain.fixture.toml index
```

## Connect an agent (MCP)

**stdio (default)** — see [`docs/MCP.md`](./MCP.md).

**HTTP streamable (local teams):**

```toml
[mcp]
transport = "http"
bind = "127.0.0.1:8765"
allow_remote = false
```

```bash
./target/release/codebrain --config ./codebrain.toml serve
# → http://127.0.0.1:8765/mcp
```

Non-loopback binds are refused unless `allow_remote = true` (see [`SECURITY.md`](./SECURITY.md)).

## Upgrading

1. Rebuild / replace the binary.
2. Run `codebrain doctor --migrate` (schema DEFINEs are idempotent on `v1`).
3. Re-run `codebrain index` if the release notes mention extractor or embedding changes.
   After enabling `embeddings.provider = fastembed` on an existing DB, use `codebrain index --force`
   (or plain `index` when the chunk table is still empty — CodeBrain auto-forces in that case).

Schema policy: [`SCHEMA.md`](./SCHEMA.md).
