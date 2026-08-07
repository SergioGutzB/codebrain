# Plan de implementación iterativo — CodeBrain v2.0

> **Modo:** Plan only (macro de producto + backlog por fase)  
> **Fuente:** Documento de Arquitectura CodeBrain v2.0 (alineado 2026-08-05)  
> **Estado del repo:** Fase 0–4 completadas (Foundation → GraphRAG)

---

## Resumen ejecutivo

CodeBrain se construye como **CLI + grafo SurrealDB + servidor MCP en Rust**, priorizando un **MVP usable en Cursor/Claude (v0.4)** con código + Obsidian, antes de embeddings avanzados y conectores SaaS. Cada fase deja un binario ejecutable y criterios de aceptación verificables; no se avanza de fase sin DoD cumplido.

**Objetivo de negocio inmediato:** un desarrollador indexa su monorepo + vault y consulta contexto cruzado desde el agente en &lt; 1 día de setup.

---

## Alcance global

| | Detalle |
|---|---------|
| **Incluye (hasta v1.0)** | Workspace Rust, SurrealDB embebido, connectors code + Obsidian, MCP stdio, GraphRAG local, watcher, ADR write-back, hardening GA |
| **No incluye (hasta v1.0)** | Notion/Confluence/Jira, cloud multi-tenant, UI web, todos los lenguajes, entity-linking LLM |
| **Invariantes** | Local-first; grafo antes que vectores; ingest incremental; secretos fuera del grafo; degradación graceful sin embeddings |

---

## Prioridades

| P | Criterio | Ejemplo |
|---|----------|---------|
| **P0** | Sin esto no hay producto | DB, schema, code index, MCP `explore_context` |
| **P1** | Diferenciador omnicanal | Obsidian wikilinks + menciones cruzadas |
| **P2** | Calidad de recuperación | Embeddings + hybrid search |
| **P3** | Ergonomía diaria | Watcher, write-back ADR |
| **P4** | Expansión | SaaS connectors |

---

## Backlog de fases (orden fijo)

| Orden | Versión | Nombre | Prioridad | Depende de | Estimación* |
|-------|---------|--------|-----------|------------|-------------|
| 0 | v0.1 | Foundation | P0 | — | 3–5 d |
| 1 | v0.2 | Code Graph | P0 | 0 | 5–8 d |
| 2 | v0.3 | Obsidian Graph | P1 | 0 | 4–6 d |
| 3 | v0.4 | MCP Brain | P0 | 1, 2 | 4–6 d |
| 4 | v0.5 | GraphRAG | P2 | 3 | 4–6 d |
| 5 | v0.6 | Watch + ADR | P3 | 3 | 3–5 d |
| 6 | v1.0 | GA local | P0 | 4, 5 | 5–8 d |
| 7 | v1.1 | SaaS omnicanal | P4 | 6 | 8–12 d |

\*Días-persona asumiendo 1 ingeniero Rust senior a tiempo parcial/completo. Ajustar ×1.5–2 con aprendizaje SurrealDB/MCP.

**Camino crítico a producto usable:** `0 → 1 → 2 → 3` (≈ 3–4 semanas). Las fases 1 y 2 pueden solaparse tras completar el trait `Connector` en fase 0.

---

## Fase 0 — Foundation (v0.1) ✅ COMPLETADA

### Objetivo
Esqueleto compilable, DB embebida, schema versionado, CLI operativa.

### Entregables
- [x] Workspace Cargo con crates vacíos/cableados.
- [x] `schemas/v1.surql` aplicado en `codebrain init`.
- [x] `codebrain doctor` (paths, DB open/close).
- [x] `codebrain.example.toml` + carga de config.
- [x] README mínimo de build (`cargo build -p codebrain-cli`).
- [x] Test de migrate idempotente + CLI smoke (`init` / `doctor` / `status`).

### Estado
Implementado el 2026-08-05. Siguiente: **Fase 1 — Code Graph**.

### Tareas
| # | Tarea | Est. | DoD parcial |
|---|-------|------|-------------|
| 0.1 | Crear workspace + crates | S | `cargo check` ok |
| 0.2 | `codebrain-db`: client Surreal embedded + migrate | M | schema aplicado |
| 0.3 | Config TOML (`figment`) | S | lee example |
| 0.4 | CLI `init` / `doctor` / `status` | M | comandos verdes |
| 0.5 | Tracing + errores tipados base | S | logs estructurados |
| 0.6 | Test integration: open DB + DEFINE | S | CI local |

### Criterios de aceptación
| ID | Criterio | Verificación |
|----|----------|--------------|
| F0-C01 | `codebrain init` crea dir de datos y aplica schema | test + manual |
| F0-C02 | `doctor` reporta OK/FAIL accionable | snapshot de salida |
| F0-C03 | Re-ejecutar migrate es idempotente | test |

### Riesgos
- API SurrealDB Rust en movimiento → pin de versión mayor y smoke test en CI.

---

## Fase 1 — Code Graph (v0.2) ✅ COMPLETADA

### Objetivo
Indexar un repo y consultar símbolos/relaciones sin MCP aún (vía CLI/`codebrain-core`).

### Entregables
- [x] `Connector` trait + `CodeConnector` modular (Rust, TS/TSX, Python, Ruby).
- [x] Persistencia transaccional `source`, `file`, `symbol`, `DEFINES`, `IMPORTS`, `CALLS`.
- [x] CLI `codebrain index --source <name>`.
- [x] Fixture E2E `testdata/repo-mini`.
- [x] Hash BLAKE3 incremental, borrado de archivos obsoletos y extracción concurrente acotada.

### Estado
Implementado el 2026-08-05. `postcheck` completo en verde. Siguiente: **Fase 2 — Obsidian Graph** (completada).

### Tareas
| # | Tarea | Est. | Depende |
|---|-------|------|---------|
| 1.1 | Trait `Connector` + `ExtractBatch` | M | 0 |
| 1.2 | Walk + gitignore/excludes | S | 1.1 |
| 1.3 | Tree-sitter Rust extractor | M | 1.1 |
| 1.4 | Tree-sitter TS/TSX + Python + Ruby | M | 1.3 |
| 1.5 | Resolver IMPORTS intra-repo | M | 1.3 |
| 1.6 | CALLS same-file + por nombre exportado | M | 1.5 |
| 1.7 | Persistencia atómica por batch + content_hash | M | 1.2 |
| 1.8 | CLI index + `status` con conteos | S | 1.7 |
| 1.9 | Tests fixture | M | 1.7 |

### Criterios de aceptación
| ID | Criterio | Verificación |
|----|----------|--------------|
| F1-C01 | Indexa `testdata/repo-mini` sin error | integration |
| F1-C02 | Símbolos públicos Rust tienen FQN único | assert DB |
| F1-C03 | Re-index sin cambios = 0 writes | métrica/checkpoint |
| F1-C04 | `IMPORTS` presentes en fixture conocida | graph query |

### Fuera de alcance de la fase
- Embeddings, MCP, Obsidian, resolución CALLS perfecta cross-crate.

---

## Fase 2 — Obsidian Graph (v0.3) ✅ COMPLETADA

### Objetivo
Indexar un vault y materializar wikilinks + menciones a símbolos (si el code graph existe).

### Entregables
- [x] `ObsidianConnector`.
- [x] Nodos `document`, aristas `REFERENCES`, `MENTIONS` con `confidence`.
- [x] Fixture `testdata/vault-mini`.
- [x] Linker de menciones cross-channel + resolución wikilink (path → title → alias).
- [x] Links rotos contabilizados sin panic (`broken_links`).

### Estado
Implementado el 2026-08-05. `postcheck` completo en verde. Siguiente: **Fase 3 — MCP Brain** (completada).

### Tareas
| # | Tarea | Est. | Depende |
|---|-------|------|---------|
| 2.1 | Frontmatter + body parse | S | 0 |
| 2.2 | Extracción wikilinks + resolución | M | 2.1 |
| 2.3 | Persist `document` + `REFERENCES` | M | 2.2 |
| 2.4 | Linker de menciones a `symbol` | M | 1.x, 2.3 |
| 2.5 | Tests vault fixture | S | 2.4 |

### Criterios de aceptación
| ID | Criterio | Verificación |
|----|----------|--------------|
| F2-C01 | `[[Nota B]]` crea `REFERENCES` | graph query |
| F2-C02 | Mención a símbolo indexado crea `MENTIONS` | assert confidence |
| F2-C03 | Notas huérfanas (link roto) se registran sin panic | log + test |

### Notas de diseño
- `auto_promote_explains = false`: solo `MENTIONS` en esta fase.
- Resolver wikilinks por: path exacto → título → alias.

---

## Fase 3 — MCP Brain (v0.4) ★ producto usable ✅ COMPLETADA

### Objetivo
Exponer el grafo a Cursor/Claude vía MCP stdio.

### Entregables
- [x] Crate `codebrain-mcp` con `rmcp` 3.1 (stdio).
- [x] Tools: `explore_context`, `graph_neighbors`, `search_symbols`, `list_sources`.
- [x] Resources: `codebrain://status`.
- [x] Guía de integración Cursor/Claude en `docs/MCP.md`.
- [x] CLI `codebrain serve`.
- [x] `QueryBudget` (nodos/vecinos/profundidad) con recorte y flag `truncated`.
- [x] Contract tests con cliente MCP real sobre transporte en memoria.

### Estado
Implementado el 2026-08-05. `postcheck` completo en verde. Siguiente: **Fase 4 — GraphRAG** (completada).

### Tareas
| # | Tarea | Est. | Depende |
|---|-------|------|---------|
| 3.1 | Bootstrap servidor `rmcp` stdio | M | 0 |
| 3.2 | Query service en `codebrain-core` | M | 1, 2 |
| 3.3 | Tool `explore_context` | M | 3.2 |
| 3.4 | Tool `graph_neighbors` + límites depth/nodes | S | 3.2 |
| 3.5 | Tool `search_symbols` (FTS) | M | 1 |
| 3.6 | Tool `list_sources` + resource status | S | 0 |
| 3.7 | Truncado de respuestas (budget tokens) | S | 3.3 |
| 3.8 | Contract tests MCP (smoke) | M | 3.3 |
| 3.9 | Docs integración Cursor/Claude | S | 3.1 |

### Criterios de aceptación
| ID | Criterio | Verificación |
|----|----------|--------------|
| F3-C01 | Agente lista tools al conectar | manual + smoke |
| F3-C02 | `explore_context` devuelve código + docs relacionados | E2E fixture |
| F3-C03 | Respuesta respeta límite de nodos configurado | unit |
| F3-C04 | Sin DB → error MCP claro, no hang | test |

### Definition of Done (release interno “dogfood”)
- [ ] Index repo real del autor + vault personal  
- [ ] Uso diario ≥ 3 días en Cursor  
- [ ] Issues P0 de usabilidad cerrados  

---

## Fase 4 — GraphRAG (v0.5) ✅ COMPLETADA

### Objetivo
Búsqueda semántica híbrida sin romper el modo grafo-only.

### Entregables
- [x] Trait `Embedder` + impl `fastembed` / `openai_compatible` / `none` (+ `hash` para tests).
- [x] Chunking de símbolos/documentos + HNSW dinámico por dimensión.
- [x] Tool MCP `semantic_search` con fusión vector + FTS + graph boost.
- [x] Degradación a FTS cuando `embeddings.provider = none`.
- [x] `doctor` valida dimensión registrada vs config.

### Estado
Implementado el 2026-08-05. `postcheck` completo en verde. Siguiente: **Fase 6 — GA local**.

### Tareas
| # | Tarea | Est. | Depende |
|---|-------|------|---------|
| 4.1 | Trait Embedder + factory por config | S | 0 |
| 4.2 | fastembed MiniLM + descarga/cache modelo | M | 4.1 |
| 4.3 | Chunking al indexar + backfill job | M | 1, 2 |
| 4.4 | Query KNN + fusión de scores | M | 4.3 |
| 4.5 | Tool `semantic_search` | S | 4.4 |
| 4.6 | OpenAI-compatible provider | S | 4.1 |
| 4.7 | `doctor` valida dimensión vs índice | S | 4.3 |

### Criterios de aceptación
| ID | Criterio | Verificación |
|----|----------|--------------|
| F4-C01 | Query semántica encuentra nota sin match exacto de nombre | fixture |
| F4-C02 | Con `provider=none`, tool no falla (FTS) | test |
| F4-C03 | Cambio de dimensión detectado por `doctor` | test |

---

## Fase 5 — Watch + ADR write-back (v0.6) ✅ COMPLETADA

### Objetivo
Ergonomía diaria: reindex automático y captura de decisiones desde el agente.

### Entregables
- [x] Watcher (`notify`) → cola de reindex parcial (debounced).
- [x] Tool `add_architectural_decision` (+ escritura `.md` opt-in).
- [x] Aristas `ABOUT`.
- [x] CLI `codebrain watch` y `index.watch` en `serve`.

### Estado
Implementado el 2026-08-05. Criterios F5-C01..C03 cubiertos por tests.

### Tareas
| # | Tarea | Est. |
|---|-------|------|
| 5.1 | Debounced file watch por source | M |
| 5.2 | Job queue no bloqueante del MCP | M |
| 5.3 | Tool ADR → DB | S |
| 5.4 | Write vault opt-in + template MD | S |
| 5.5 | Tests de debounce / crash mid-job | M |

### Criterios de aceptación
| ID | Criterio | Verificación |
|----|----------|--------------|
| F5-C01 | Editar un `.md` reindexa solo ese documento | integration (`reindex_source_paths`) |
| F5-C02 | ADR aparece en `explore_context` / neighbors del símbolo `about` | E2E MCP |
| F5-C03 | `write_vault=false` no toca el filesystem del vault | test |

---

## Fase 6 — GA local (v1.0) ✅ COMPLETADA

### Objetivo
Calidad de producción para uso personal/equipo local.

### Entregables
- [x] Promoción controlada `MENTIONS` → `EXPLAINS` (`promote_mention`).
- [x] HTTP streamable MCP opcional (`mcp.transport = "http"`).
- [x] Bench documentado (`docs/BENCH.md` + `scripts/bench-index.sh`); CI matrix; release on tag.
- [x] Documentación de instalación, seguridad y troubleshooting.
- [x] Política de versionado de schema (`docs/SCHEMA.md`).

### Estado
Implementado el 2026-08-05. Versión workspace `1.0.0`. Siguiente opcional: **Fase 7 — SaaS**.

### Checklist DoD GA
- [x] Bench: metodología + script sintético 1k/2k; referencia M5 ≈57 s cold / warm ≪1–2 s (stretch &lt;30 s documentado)
- [x] Coverage en parsers / query / MCP contract (suite existente + promote)
- [x] README + `doctor` + INSTALL/TROUBLESHOOTING
- [x] Changelog / semver (`CHANGELOG.md`, `1.0.0`)
- [x] Threat model breve (`docs/SECURITY.md`)

---

## Fase 7 — SaaS omnicanal (v1.1+) — Jira ✅ · Confluence ✅ · Notion ✅

### Objetivo
Notion / Confluence / Jira como `document` + aristas `RESOLVES` / cross-refs.

### Orden sugerido de conectores
1. **Jira** (tickets ↔ símbolos por key en código) ✅  
2. **Confluence** (páginas de diseño + menciones + refs a tickets) ✅  
3. **Notion** (workspaces personales/equipo) ✅

### Entregables
- [x] Auth tokens via env (`JIRA_*` Atlassian · `NOTION_TOKEN`).
- [x] Sync incremental por `content_hash` (updated+body) + rate limit básico.
- [x] Index path `kind = jira` + linker `RESOLVES`.
- [x] Index path `kind = confluence` + mentions + refs a Jira.
- [x] Index path `kind = notion` + mentions + refs a Jira.
- [x] Tools MCP filtrar por `source.kind` (`source_kinds`).

### Fuera de alcance inicial
- Write-back a Jira/Notion/Confluence (solo lectura en v1.1).

Docs: [`docs/JIRA.md`](./docs/JIRA.md) · [`docs/CONFLUENCE.md`](./docs/CONFLUENCE.md) · [`docs/NOTION.md`](./docs/NOTION.md) · [`docs/KNOWLEDGE_GRAPH.md`](./docs/KNOWLEDGE_GRAPH.md) · [`docs/BACKLOG.md`](./docs/BACKLOG.md).

---

## Estrategia de testing (transversal)

| Capa | Qué | Dónde |
|------|-----|-------|
| Unit | parsers wikilink, FQN, chunking, score fusion | `codebrain-connector-*`, `core` |
| Integration | Surreal embedded + schema + index fixture | `tests/integration` |
| Contract | MCP tools schemas + smoke invoke | `codebrain-mcp` |
| E2E manual | Cursor real contra fixture | checklist por release |
| Bench | index throughput + query p95 | `benches/` |

Cada criterio `F*-C**` debe mapear a al menos un test automatizado salvo los marcados “manual”.

---

## Estructura de trabajo recomendada

```text
Semana 1      → Fase 0 + inicio Fase 1 (trait + Rust parser)
Semana 2      → Fase 1 completa + inicio Fase 2
Semana 3      → Fase 2 + Fase 3 (MCP)
Semana 4      → Dogfood v0.4 + bugs P0
Semanas 5–6   → Fase 4 (GraphRAG)
Semana 7      → Fase 5 (watch + ADR)
Semanas 8–9   → Fase 6 (GA)
Después       → Fase 7 bajo demanda
```

---

## Decisiones abiertas (cerrar antes de implementar)

| # | Pregunta | Default propuesto | Impacto |
|---|----------|-------------------|---------|
| D1 | ¿Nombre del binario y crate público? | `codebrain` | branding/publish |
| D2 | ¿Licencia? | MIT o Apache-2.0 | distribución |
| D3 | ¿Soportar solo macOS al inicio o también Linux? | macOS + Linux desde v0.2 | CI matrix |
| D4 | ¿Modelo embeddings default offline obligatorio en release? | sí, con download lazy en primer `index` | UX/offline |
| D5 | ¿Monorepo de usuario piloto? | el del autor (dogfood) | fixtures reales |

---

## Próximo paso sugerido

**Próximo paso sugerido:** dogfood Notion (`NOTION_TOKEN`), embeddings `fastembed`, o esperar binarios del workflow `release` en el tag `v1.1.0`.
