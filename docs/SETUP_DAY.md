# CodeBrain — un día de setup

Guía práctica para pasar de cero a **grafo usable en el agente** en una jornada.
Detalle técnico: [`INSTALL.md`](./INSTALL.md) · mapa mental: [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md).

Estimación: **2–4 h** el primer día (más si activas SaaS + embeddings).

---

## Mañana — binario, config, primer grafo local

### 1. Compilar (15–40 min la primera vez)

```bash
git clone https://github.com/SergioGutzB/codebrain.git   # o tu fork
cd codebrain
cargo build -p codebrain-cli --release
```

Binary: `./target/release/codebrain`.

### 2. Config mínima (10 min)

```bash
./target/release/codebrain init --config ./codebrain.toml
```

Edita `codebrain.toml` (está en `.gitignore` — no lo subas):

```toml
[database]
path = "~/.local/share/codebrain/db"

[sources.backend]
kind = "git_repo"
path = "~/path/to/your-service"
languages = ["ruby"]          # o rust / typescript / python

[sources.notes]
kind = "obsidian_vault"
path = "~/Obsidian Vault"

[embeddings]
provider = "none"             # lo activamos después
```

Plantilla completa: [`codebrain.example.toml`](../codebrain.example.toml).

### 3. Salud + índice local (10–30 min)

```bash
./target/release/codebrain --config ./codebrain.toml doctor --migrate
./target/release/codebrain --config ./codebrain.toml index
./target/release/codebrain --config ./codebrain.toml status
```

Esperado: `file` / `symbol` / `document` &gt; 0. `chunk` puede ser 0 con `provider = none`.

Sin repos personales, smoke con fixtures:

```bash
cargo run -p codebrain-cli -- --config testdata/codebrain.fixture.toml index
```

---

## Mediodía — conectar el agente (MCP)

### 4. Cursor / Claude Code (15 min)

Arranque (el cliente lo lanza solo si está en `mcp.json`):

```bash
./target/release/codebrain --config ./codebrain.toml serve
```

En `~/.cursor/mcp.json` (rutas **absolutas**):

```json
{
  "mcpServers": {
    "codebrain": {
      "command": "/ABS/path/to/codebrain/target/release/codebrain",
      "args": ["--config", "/ABS/path/to/codebrain/codebrain.toml", "serve"]
    }
  }
}
```

Guía completa de tools y tokens: [`MCP.md`](./MCP.md).

### 5. Primeras preguntas al grafo (15 min)

Desde el agente, en orden útil:

| Objetivo | Tool |
|----------|------|
| ¿Qué fuentes hay? | `list_sources` |
| Buscar un símbolo | `search_symbols` |
| Tema omnicanal | `explore_context` |
| Pregunta en lenguaje natural | `semantic_search` (mejor con embeddings) |
| Expandir un nodo | `graph_neighbors` con `kind:source:key` |
| Leyenda nodos/aristas | resource `codebrain://schema` |

Mapa de nodos (`symbol`, `document`, `resolves`, `mentions`, …): [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md).

---

## Tarde — embeddings + SaaS (opcionales)

### 6. `fastembed` para `semantic_search` real (30–90 min)

```toml
[embeddings]
provider = "fastembed"
model = "all-MiniLM-L6-v2"
dimension = 384
```

```bash
# Cache del modelo ONNX (recomendado si el cliente Rust falla al descargar):
export HF_HOME="${HF_HOME:-$HOME/.cache/fastembed}"
export FASTEMBED_CACHE_DIR="${FASTEMBED_CACHE_DIR:-$HOME/.cache/fastembed}"

./target/release/codebrain --config ./codebrain.toml index --force
./target/release/codebrain --config ./codebrain.toml doctor   # embeddings.dimension ok
./target/release/codebrain --config ./codebrain.toml status   # chunk > 0
```

Números de referencia y tips: [`BENCH.md`](./BENCH.md) · problemas: [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md).

Si un DB viejo falla al crear el índice vectorial, apunta `database.path` a un directorio nuevo y reindexa.

### 7. Jira / Confluence / Notion (30–60 min)

Solo si ya tienes tokens. Nunca los commits.

| Canal | Env | Doc |
|-------|-----|-----|
| Jira + Confluence | `JIRA_BASE_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN` | [`JIRA.md`](./JIRA.md) · [`CONFLUENCE.md`](./CONFLUENCE.md) |
| Notion | `NOTION_TOKEN` (+ compartir páginas con la integración) | [`NOTION.md`](./NOTION.md) |

```bash
./target/release/codebrain --config ./codebrain.toml doctor
./target/release/codebrain --config ./codebrain.toml index --source tickets
./target/release/codebrain --config ./codebrain.toml index --source wiki
# Notion solo si doctor dice auth ok:
./target/release/codebrain --config ./codebrain.toml index --source notion
```

Jira guarda un cursor `updated` (reindex barato). Full refresh: `index --source tickets --force`.

---

## Checklist de cierre del día

- [ ] `doctor` healthy (sin `FAIL`)
- [ ] `status` muestra símbolos y/o documentos
- [ ] MCP responde `list_sources` / `explore_context` desde el IDE
- [ ] (opc) `chunk` &gt; 0 con `fastembed`
- [ ] (opc) al menos un ticket Jira o página Confluence/Notion en el grafo

## Qué leer después

| Doc | Para qué |
|-----|----------|
| [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md) | Crates, features, nodos/aristas |
| [`MCP.md`](./MCP.md) | Tools, filtros `source_kinds`, tokens |
| [`SECURITY.md`](./SECURITY.md) | Secretos, HTTP MCP, threat model |
| [`BACKLOG.md`](./BACKLOG.md) | Qué falta / prioridades |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | Cuando algo no indexa o no busca |
