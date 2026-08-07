# CodeBrain

Grafo de conocimiento omnicanal (código + Obsidian → MCP) construido en Rust.

> Estado actual: **v1.1 SaaS** — GA local + Jira + Confluence + Notion.

## Requisitos

- Rust 1.85+ (`rustup`)
- macOS o Linux

## Agent contract (all tools)

Canonical instructions for Cursor / Claude Code / OpenCode / others:

- **[AGENTS.md](./AGENTS.md)** — start here
- Pre: `./scripts/agent/preflight.sh` or `just pre`
- Post: `./scripts/agent/postcheck.sh` or `just post`
- Commits: Conventional Commits, **no** `Co-Authored-By` (see `docs/agent/COMMIT_CONVENTION.md`)
- Install git hook: `./scripts/agent/install-hooks.sh`

## Quick start

Guía paso a paso de un día (local → MCP → embeddings → SaaS): [`docs/SETUP_DAY.md`](./docs/SETUP_DAY.md).

```bash
# Compilar
cargo build -p codebrain-cli --release

# Inicializar DB + config de ejemplo
./target/release/codebrain init --config ./codebrain.toml

# Health check
./target/release/codebrain doctor --migrate

# Conteos
./target/release/codebrain status --config ./codebrain.toml
```

Edita `codebrain.toml`, añade fuentes `git_repo` y/o `obsidian_vault` y ejecuta:

```bash
./target/release/codebrain --config ./codebrain.toml index
# una fuente concreta:
./target/release/codebrain --config ./codebrain.toml index --source backend
./target/release/codebrain --config ./codebrain.toml index --source notes

# watcher dedicado (o `index.watch = true` en serve):
./target/release/codebrain --config ./codebrain.toml watch
```

El primer índice persiste archivos/símbolos (código) o documentos/wikilinks/menciones (vault).
Ejecuciones sin cambios reportan `indexed=0` gracias a hashes BLAKE3.
El watcher reindexa solo las rutas tocadas (debounce configurable).

Smoke test con fixtures (repo + vault):

```bash
cargo run -p codebrain-cli -- --config testdata/codebrain.fixture.toml index
```

## Usarlo desde Cursor / Claude Code

```bash
./target/release/codebrain --config ./codebrain.toml serve
```

Configuración de cliente, tools disponibles y formato de tokens de nodo: [`docs/MCP.md`](./docs/MCP.md).

## Workspace

| Crate | Rol |
|-------|-----|
| `codebrain-cli` | Binario `codebrain` |
| `codebrain-core` | Config, doctor, indexer, linker, watch, ADR |
| `codebrain-db` | SurrealDB + schema + graph/docs |
| `codebrain-connector` | Trait `Connector` |
| `codebrain-connector-code` | Tree-sitter (Rust/TS/Python/Ruby) |
| `codebrain-connector-obsidian` | Vault Obsidian (frontmatter/wikilinks) |
| `codebrain-connector-saas` | Jira + Confluence + Notion |
| `codebrain-embed` | Providers de embeddings + chunking |
| `codebrain-mcp` | Servidor MCP stdio + HTTP streamable (`rmcp`) |

## Documentación

- [Documento de Arquitectura](./Documento%20de%20Arquitectura-%20Ecosistema%20Global%20de%20Conocimiento%20Omnicanal%20(CodeBrain%20v2.0).md)
- [Plan de implementación](./PLAN-IMPLEMENTACION.md)
- [Un día de setup](./docs/SETUP_DAY.md) · [Instalación](./docs/INSTALL.md)
- [Integración MCP](./docs/MCP.md)
- [Seguridad](./docs/SECURITY.md)
- [Troubleshooting](./docs/TROUBLESHOOTING.md)
- [Schema](./docs/SCHEMA.md) · [`schemas/v1.surql`](./schemas/v1.surql)
- [Knowledge graph](./docs/KNOWLEDGE_GRAPH.md) · [Backlog](./docs/BACKLOG.md)
- [Bench](./docs/BENCH.md) · [Jira](./docs/JIRA.md) · [Confluence](./docs/CONFLUENCE.md) · [Notion](./docs/NOTION.md) · [CHANGELOG](./CHANGELOG.md)

## Licencia

MIT
