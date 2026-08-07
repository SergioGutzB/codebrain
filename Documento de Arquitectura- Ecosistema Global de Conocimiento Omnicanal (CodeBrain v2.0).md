# Documento de Arquitectura: CodeBrain v2.0

> **Estado:** alineado a producto funcional de producción (MVP → GA)  
> **Última revisión:** 2026-08-05  
> **Tipo:** arquitectura de referencia + decisiones cerradas para implementación

---

## 1. Visión

**CodeBrain** es un grafo de conocimiento local-first que unifica código fuente y bases documentales (Obsidian primero; Notion / Confluence / Jira después) y lo expone a agentes LLM vía **MCP**.

El valor central no es “otro índice RAG”: es **navegación cruzada determinística** (grafo) + **recuperación semántica** (vectores) en un solo motor, de modo que un agente pueda ir de `fn authenticate` → nota `[[Diseño de Seguridad]]` → ticket `AUTH-12` sin perder trazabilidad.

### North-star (definición de éxito)

Un desarrollador conecta Cursor/Claude a CodeBrain y, en &lt;2 s, obtiene contexto accionable que mezcla:

1. símbolo de código + callers/callees  
2. notas Obsidian que lo explican  
3. (fase posterior) tickets/páginas SaaS vinculados  

---

## 2. Principios de diseño

| Principio | Implicación |
|-----------|-------------|
| **Local-first** | SurrealDB embebido por defecto; datos no salen de la máquina salvo conectores SaaS opt-in |
| **Omnicanalidad estructurada** | Repos, vaults y wikis son ciudadanos de primera clase con el mismo modelo de entidad/arista |
| **Grafo antes que embeddings** | Las aristas exactas (`CALLS`, `REFERENCES`, `EXPLAINS`) son la fuente de verdad; los vectores son fallback |
| **Ingestión incremental** | Hash de contenido + watcher; nunca reindexar todo en caliente salvo bootstrap |
| **Conectores pluggables** | Trait `Connector` estable; nuevos orígenes sin tocar el core |
| **MCP como API de producto** | La UI primaria es el agente; CLI es operativa (index, status, doctor) |
| **Rust-native** | Throughput y footprint bajos; un binario (`codebrain`) + crates internos |

### Anti-principios (qué no somos)

- No somos un IDE ni un reemplazo de GitHub Copilot indexing cloud.
- No somos un CMS de documentación.
- No prometemos entity-linking “perfecto” LLM-based en v1 (heurísticas + confianza primero).

---

## 3. Alcance por versión

### v0.1 — Foundation (no es producto aún)

- Workspace Cargo, config, SurrealDB embebido, esquema, CLI `init` / `doctor`.

### v0.2 — Code Graph MVP

- Connector de código (`tree-sitter`) para **Rust, TypeScript/TSX, Python, Ruby**.
- Nodos: `repository`, `file`, `symbol` (función/clase/módulo unificados).
- Aristas: `CONTAINS`, `DEFINES`, `IMPORTS`, `CALLS` (best-effort).

### v0.3 — Obsidian Graph

- Connector de vault: frontmatter, headings, `[[wikilinks]]`, tags, aliases.
- Aristas: `REFERENCES` (wikilink), menciones textuales a símbolos indexados → `MENTIONS` (candidato a `EXPLAINS`).

### v0.4 — MCP Brain (producto usable)

- Servidor MCP (`rmcp`) sobre stdio (Cursor/Claude Code).
- Tools: `explore_context`, `graph_neighbors`, `search_symbols`, `list_sources`.
- Entregable: un desarrollador puede usarlo diariamente en un monorepo + vault.

### v0.5 — GraphRAG híbrido

- Embeddings locales (`fastembed` / `ort`, modelo `all-MiniLM-L6-v2`, 384-d).
- Fallback opcional OpenAI-compatible (`text-embedding-3-small`) vía config.
- Tool: `semantic_search` + query SurrealQL KNN + expansión de grafo.

### v0.6 — Write-back & watch

- File watcher + reindex parcial.
- Tool `add_architectural_decision` → nodo `architecture_decision` + opcional `.md` en vault.

### v1.0 — GA local

- Hardening, métricas, tests de carga, docs de instalación, profiles de config.
- Entity linking con score de confianza y revisión via MCP resource.

### v1.1+ — Omnicanal SaaS

- Conectores Notion, Confluence, Jira (opt-in, tokens en keychain/env).
- Aristas `RESOLVES`, `DOCUMENTS` cross-source.

### Fuera de alcance (explícito hasta v1.0)

- Multi-tenant cloud hosted / SaaS de CodeBrain.
- Fine-tuning de modelos.
- UI web rica (solo status CLI + MCP resources).
- Indexación de binarios, imágenes OCR, video.
- Soportar todos los lenguajes tree-sitter desde el día 1.

---

## 4. Stack tecnológico (decisiones cerradas)

| Componente | Tecnología | Decisión |
|------------|------------|----------|
| Runtime | Rust 1.85+, Tokio | Edición 2024 |
| DB | `surrealdb` (embedded RocksDB; remote opcional) | Grafo + vector + FTS en un solo store |
| MCP | **`rmcp`** (SDK oficial, no `mcp-sdk`) | stdio primero; HTTP streamable en v1.0 |
| Código | `tree-sitter` + grammars por lenguaje | Extracción AST, no regex |
| Markdown / Obsidian | `pulldown-cmark` + `gray_matter` + parser wikilinks propio | Wikilinks no son Markdown estándar |
| SaaS (post-v1) | `reqwest` + `serde` | Sync incremental por `updated_at` / cursor |
| Embeddings | **Primario:** `fastembed` (ONNX/`ort`). **Alt:** API OpenAI-compatible | Candle queda como spike, no default prod |
| CLI | `clap` | Subcomandos `init`, `index`, `serve`, `doctor`, `status` |
| Config | `figment` / TOML (`codebrain.toml`) | Jerarquía: flags > env > archivo > defaults |
| Observabilidad | `tracing` + `tracing-subscriber` | Spans por connector e index job |
| Errores | `thiserror` + `anyhow` en bins | Errores tipados en libs |

### Decisiones explícitas vs documento original

1. **`mcp-sdk` → `rmcp`:** el crate oficial es `rmcp` (MCP 2026-07-28).
2. **Tablas tipadas por SaaS se difieren:** en v0.x usamos `document` + `source_kind`; tablas `notion_page` / `jira_ticket` aparecen en v1.1.
3. **Símbolos unificados:** `symbol` con `kind` (`function` \| `class` \| `module` \| `interface` \| …) en lugar de tablas separadas prematuras.
4. **Embeddings:** default local 384-d; dimensión configurable; índice HNSW alineado al modelo activo.
5. **EXPLAINS no se inventa en ingest ciego:** se materializa cuando `confidence >= umbral` (default 0.75) desde `MENTIONS`.

---

## 5. Arquitectura del sistema

```
┌─────────────────────────────────────────────────────────────────┐
│  Agentes (Cursor, Claude Code, …)                               │
│                         MCP (stdio / HTTP)                      │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│  codebrain-mcp          Tools + Resources + Prompts             │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│  codebrain-core                                                 │
│  · Query service (graph walk, hybrid search)                    │
│  · Index orchestrator (jobs, hashes, checkpoints)               │
│  · Entity linker (menciones → EXPLAINS)                         │
└───────┬─────────────────────┬─────────────────────┬─────────────┘
        │                     │                     │
┌───────▼───────┐   ┌─────────▼─────────┐   ┌───────▼───────────┐
│ codebrain-    │   │ codebrain-        │   │ codebrain-        │
│ connector-code│   │ connector-obsidian│   │ connector-saas*   │
└───────┬───────┘   └─────────┬─────────┘   └───────┬───────────┘
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              │
                    ┌─────────▼─────────┐
                    │  SurrealDB        │
                    │  graph + HNSW +   │
                    │  full-text        │
                    └───────────────────┘
* v1.1+
```

### Crates (workspace)

```
codebrain/
├── Cargo.toml                 # workspace
├── crates/
│   ├── codebrain-cli/         # bin: codebrain
│   ├── codebrain-core/        # dominio, queries, linker, embeddings trait
│   ├── codebrain-db/          # schema SurrealQL, migraciones, client
│   ├── codebrain-mcp/         # servidor MCP
│   ├── codebrain-connector/   # trait Connector + tipos de ingest
│   ├── codebrain-connector-code/
│   ├── codebrain-connector-obsidian/
│   └── codebrain-connector-saas/   # feature-gated / crate vacío hasta v1.1
├── schemas/
│   └── v1.surql
├── codebrain.example.toml
└── docs/
```

### Trait de conector (contrato estable)

```rust
#[async_trait]
pub trait Connector: Send + Sync {
    fn id(&self) -> &str;
    fn source_kind(&self) -> SourceKind;

    /// Diff incremental: solo unidades cuyo content_hash cambió.
    async fn discover(&self, ctx: &IndexContext) -> Result<Vec<WorkItem>>;

    async fn extract(&self, item: &WorkItem) -> Result<ExtractBatch>;
}
```

`ExtractBatch` contiene nodos tipados + aristas candidatas + chunks embedibles. El orchestrator persiste de forma atómica por batch.

---

## 6. Modelo de datos (SurrealQL)

Principios: `SCHEMAFULL`, IDs estables derivados de contenido (`source` + path + FQN), embeddings opcionales, índices FTS + HNSW.

```surql
-- ===== Fuentes =====
DEFINE TABLE source SCHEMAFULL;
DEFINE FIELD kind         ON source TYPE string ASSERT $value IN ['git_repo', 'obsidian_vault', 'notion', 'confluence', 'jira'];
DEFINE FIELD name         ON source TYPE string;
DEFINE FIELD root_path    ON source TYPE option<string>;
DEFINE FIELD remote_url   ON source TYPE option<string>;
DEFINE FIELD last_indexed ON source TYPE option<datetime>;
DEFINE FIELD meta         ON source TYPE object FLEXIBLE;

-- ===== Código =====
DEFINE TABLE file SCHEMAFULL;
DEFINE FIELD source       ON file TYPE record<source>;
DEFINE FIELD path         ON file TYPE string;
DEFINE FIELD language     ON file TYPE option<string>;
DEFINE FIELD content_hash ON file TYPE string;
DEFINE FIELD mtime        ON file TYPE datetime;
DEFINE FIELD embedding    ON file TYPE option<array<float>>;

DEFINE TABLE symbol SCHEMAFULL;
DEFINE FIELD source       ON symbol TYPE record<source>;
DEFINE FIELD file         ON symbol TYPE record<file>;
DEFINE FIELD name         ON symbol TYPE string;
DEFINE FIELD fqn          ON symbol TYPE string;          -- p.ej. crate::auth::login
DEFINE FIELD kind         ON symbol TYPE string;          -- function|class|module|...
DEFINE FIELD signature    ON symbol TYPE option<string>;
DEFINE FIELD start_line   ON symbol TYPE int;
DEFINE FIELD end_line     ON symbol TYPE int;
DEFINE FIELD content_hash ON symbol TYPE string;
DEFINE FIELD embedding    ON symbol TYPE option<array<float>>;
DEFINE INDEX symbol_fqn   ON symbol FIELDS fqn UNIQUE;
DEFINE INDEX symbol_name  ON symbol FIELDS name;
DEFINE ANALYZER codebrain_analyzer TOKENIZERS blank, class FILTERS lowercase, edgengram(2,12);
DEFINE INDEX symbol_fts   ON symbol FIELDS name, fqn, signature SEARCH ANALYZER codebrain_analyzer BM25;

-- ===== Documentos (Obsidian ahora; SaaS después) =====
DEFINE TABLE document SCHEMAFULL;
DEFINE FIELD source       ON document TYPE record<source>;
DEFINE FIELD path         ON document TYPE string;        -- relativo al vault o URL/id remoto
DEFINE FIELD title        ON document TYPE string;
DEFINE FIELD aliases      ON document TYPE array<string>;
DEFINE FIELD tags         ON document TYPE array<string>;
DEFINE FIELD body         ON document TYPE string;
DEFINE FIELD content_hash ON document TYPE string;
DEFINE FIELD updated_at   ON document TYPE datetime;
DEFINE FIELD embedding    ON document TYPE option<array<float>>;
DEFINE INDEX document_title ON document FIELDS title;
DEFINE INDEX document_fts   ON document FIELDS title, body, aliases SEARCH ANALYZER codebrain_analyzer BM25;

-- Chunks embedibles (granularidad de búsqueda semántica)
DEFINE TABLE chunk SCHEMAFULL;
DEFINE FIELD parent       ON chunk TYPE record;           -- file|symbol|document|architecture_decision
DEFINE FIELD ordinal      ON chunk TYPE int;
DEFINE FIELD text         ON chunk TYPE string;
DEFINE FIELD embedding    ON chunk TYPE option<array<float>>;
DEFINE INDEX chunk_vec ON chunk FIELDS embedding HNSW DIMENSION 384 DIST COSINE;

-- ===== Decisiones de arquitectura (write-back) =====
DEFINE TABLE architecture_decision SCHEMAFULL;
DEFINE FIELD title        ON architecture_decision TYPE string;
DEFINE FIELD body         ON architecture_decision TYPE string;
DEFINE FIELD created_at   ON architecture_decision TYPE datetime;
DEFINE FIELD created_by   ON architecture_decision TYPE string; -- 'agent' | user id
DEFINE FIELD vault_path   ON architecture_decision TYPE option<string>;
DEFINE FIELD embedding    ON architecture_decision TYPE option<array<float>>;

-- ===== Relaciones =====
DEFINE TABLE CONTAINS  TYPE RELATION FROM source TO (file, document);
DEFINE TABLE DEFINES   TYPE RELATION FROM file TO symbol;
DEFINE TABLE IMPORTS   TYPE RELATION FROM (file, symbol) TO (file, symbol);
DEFINE TABLE CALLS     TYPE RELATION FROM symbol TO symbol;
DEFINE TABLE REFERENCES TYPE RELATION FROM document TO document;  -- [[wikilinks]]
DEFINE TABLE MENTIONS  TYPE RELATION FROM document TO symbol;     -- candidato
DEFINE FIELD confidence ON MENTIONS TYPE float;
DEFINE FIELD evidence   ON MENTIONS TYPE option<string>;

DEFINE TABLE EXPLAINS  TYPE RELATION FROM (document, architecture_decision) TO symbol;
DEFINE FIELD confidence ON EXPLAINS TYPE float;
DEFINE FIELD promoted_from ON EXPLAINS TYPE option<record<MENTIONS>>;

DEFINE TABLE RESOLVES  TYPE RELATION FROM symbol TO document;     -- código → ticket/doc (v1.1)
DEFINE TABLE ABOUT     TYPE RELATION FROM architecture_decision TO (symbol, document, file);
```

> **Nota:** la dimensión HNSW (384) debe coincidir con el modelo activo. Si el usuario elige API 1536-d, la migración recrea el índice (documentado en `codebrain doctor`).

---

## 7. Capa de conectores

### 7.1 CodeConnector

1. Descubre archivos por globs (`**/*.{rs,ts,tsx,py}`), respeta `.gitignore` + `codebrain.toml` excludes.
2. Parsea con tree-sitter; extrae símbolos y spans.
3. Resuelve `IMPORTS` intra-repo (FQN / path); `CALLS` best-effort en el mismo archivo y, si es posible, por nombre exportado.
4. Emite chunks: docstring + signature + cuerpo truncado (límite configurable, default 2–4 KB).

### 7.2 ObsidianConnector

1. Lee `.md` del vault; parsea YAML frontmatter (`tags`, `aliases`).
2. Extrae `[[wikilink]]` y `[[wikilink|alias]]` → `REFERENCES` (resolución por título/alias/path).
3. Detecta menciones a `symbol.name` / `fqn` con word-boundary → `MENTIONS` + `confidence`.
4. No escribe en el vault salvo tool de write-back (opt-in).

### 7.3 SaaSConnector (v1.1+)

- Sync por conector con cursor/`updated_at`.
- Mapea a `document` + `source.kind`.
- Rate-limit y backoff; nunca bloquea el servidor MCP (job en background).

---

## 8. Servidor MCP (producto)

### Tools (v0.4+)

| Tool | Entrada | Salida | Fase |
|------|---------|--------|------|
| `explore_context` | `entity` (nombre/FQN/path) | nodo + vecinos (código ↔ docs) | v0.4 |
| `graph_neighbors` | `id`, `edge_types[]`, `depth` | subgrafo | v0.4 |
| `search_symbols` | `query`, `kind?` | ranking FTS | v0.4 |
| `list_sources` | — | fuentes indexadas + freshness | v0.4 |
| `semantic_search` | `query`, `limit?` | chunks + expansión grafo | v0.5 |
| `add_architectural_decision` | `title`, `body`, `about[]`, `write_vault?` | ADR creada | v0.6 |
| `promote_mention` | `mentions_id` | crea `EXPLAINS` | v1.0 |

### Resources

- `codebrain://status` — salud DB, última indexación, conteos.
- `codebrain://source/{id}` — metadata de fuente.

### Transportes

- **v0.4:** stdio (integración Cursor/Claude).
- **v1.0:** streamable HTTP opcional para equipos locales.

---

## 9. GraphRAG híbrido

Pipeline de `semantic_search`:

1. Embed de la query (mismo modelo que indexación).
2. KNN sobre `chunk` (`embedding <|k,ef|> $q`).
3. Expandir padres (`symbol` / `document`) y vecinos 1-hop (`CALLS`, `EXPLAINS`, `REFERENCES`).
4. Fusionar scores: `0.6 * vector + 0.3 * graph_boost + 0.1 * fts` (pesos en config).
5. Devolver citas con path, líneas y evidencia.

Sin embedding disponible → degradar a FTS + grafo (nunca fallar el tool).

---

## 10. Configuración de producto

```toml
# codebrain.toml
[database]
path = "~/.codebrain/db"

[sources.backend]
kind = "git_repo"
path = "~/monokera/policy-service"
languages = ["rust", "typescript"]

[sources.notes]
kind = "obsidian_vault"
path = "~/Obsidian/Engineering"

[embeddings]
provider = "fastembed"          # fastembed | openai_compatible | none
model = "all-MiniLM-L6-v2"
dimension = 384
# base_url / api_key solo si openai_compatible

[index]
watch = true
batch_size = 64
exclude = ["**/node_modules/**", "**/target/**", "**/.git/**"]

[linker]
mention_threshold = 0.75
auto_promote_explains = false

[mcp]
transport = "stdio"
```

---

## 11. Operación y calidad de producción

| Área | Requisito |
|------|-----------|
| **Idempotencia** | Re-index del mismo hash = no-op |
| **Crash safety** | Checkpoint por `source` + batch; jobs reanudables |
| **Doctor** | Verifica DB, grammars, modelo embeddings, permisos de paths |
| **Tests** | Unit (parsers), integration (Surreal embedded), contract MCP |
| **Bench** | Index de fixture 1k archivos + 2k notas &lt; 30 s en laptop ref. |
| **Seguridad** | Secretos solo env/keychain; nunca en el grafo; allowlist de paths |
| **Privacidad** | Default local; SaaS opt-in explícito |

---

## 12. Riesgos y mitigaciones

| Riesgo | Impacto | Mitigación |
|--------|---------|------------|
| `CALLS` impreciso cross-file | Medio | Marcar `confidence`; no bloquear MVP |
| Falsos `EXPLAINS` | Alto | Umbral + promoción manual; `auto_promote=false` |
| Dimensión embedding mismatch | Alto | `doctor` + migración versionada de índice |
| Vaults enormes | Medio | Chunking + exclude + index incremental |
| Tree-sitter grammar drift | Bajo | Pin de versiones de grammars en workspace |
| MCP tool overload de contexto | Medio | Límites de tokens/nodos en respuesta + paginación |

---

## 13. Criterios de aceptación del producto (Definition of Done v0.4)

1. `codebrain init && codebrain index` indexa un repo Rust + vault Obsidian de demo.
2. Cursor conectado vía MCP obtiene `explore_context("login")` con símbolo + nota relacionada si existe mención/`[[link]]`.
3. Re-index sin cambios termina en &lt;1 s (hashes).
4. `codebrain doctor` verde en máquina limpia tras README de instalación.
5. Suite CI: `cargo test` + fixture E2E de index+query.

---

## 14. Roadmap resumido

| Fase | Versión | Entregable |
|------|---------|------------|
| 0 | v0.1 | Foundation + schema + CLI |
| 1 | v0.2 | Code graph |
| 2 | v0.3 | Obsidian graph |
| 3 | v0.4 | **MCP usable en producción personal** |
| 4 | v0.5 | GraphRAG |
| 5 | v0.6 | Watch + ADR write-back |
| 6 | v1.0 | GA local |
| 7 | v1.1+ | SaaS omnicanal |

El plan de implementación detallado (tareas, estimaciones, dependencias) vive en [`PLAN-IMPLEMENTACION.md`](./PLAN-IMPLEMENTACION.md).
