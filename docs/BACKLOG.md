# CodeBrain — backlog

Lista viva de trabajo **después** de v1.1 SaaS (Jira + Confluence + Notion).
Prioridad alineada con `PLAN-IMPLEMENTACION.md`. Marcar ítems al completarlos.

## Hecho (referencia)

| Ítem | Versión |
|------|---------|
| Foundation → GA local (fases 0–6) | 1.0.0 |
| Jira + `RESOLVES` | 1.1.0 |
| Confluence + mentions + refs a Jira | 1.1.0 |
| Notion + mentions + refs a Jira | 1.1.0 |
| Docs de producto (INSTALL, MCP, SECURITY, conectores) | 1.1.0 |
| Grafo de conocimiento del repo + este backlog | 1.1.x |

---

## Ahora / próximo

| P | Ítem | Notas | Estado |
|---|------|-------|--------|
| P1 | Filtro MCP por `source_kinds` | `list_sources` / search / explore / semantic | ✅ |
| P2 | Dogfood Notion live | Requiere `NOTION_TOKEN` + páginas compartidas con la integración | pendiente (sin token local) |
| P2 | Tag / release GitHub `v1.1.0` | Necesita primer commit + remote | pendiente |
| P2 | Recurso MCP `codebrain://schema` | Leyenda nodos/aristas | ✅ |

---

## Backlog producto (post–1.1)

### Recuperación y MCP

- [x] Filtro MCP `source_kinds` (`git_repo` \| `obsidian_vault` \| `jira` \| `confluence` \| `notion`)
- [x] Filtro también por **nombre** de source (mismo campo `source_kinds`)
- [x] Recurso MCP `codebrain://schema` (leyenda nodos/aristas)
- [ ] Mejores excerpts en `semantic_search` (highlights)

### Calidad de índice

- [ ] Embeddings dogfood con `fastembed` en monorepo real + métricas en `docs/BENCH.md`
- [ ] Acercar cold index sintético 1k/2k al stretch &lt;30s (hoy ~57s en M5)
- [ ] Contar `resolves`/`references` solo en aristas **nuevas** (hoy re-cuentan en cada index)

### Conectores

- [ ] Jira: sync por `updated` cursor persistido (menos re-fetch)
- [ ] Confluence: más espacios / CQL presets en example
- [ ] Notion: databases → filas como documentos (hoy solo pages)
- [ ] Write-back opt-in a Jira/Notion/Confluence (**fuera** de v1.1 lectura)

### DX / release

- [ ] Primer commit + remote + CI verde en repo limpio
- [ ] `codebrain init` genera example con stubs SaaS comentados (ya en example.toml)
- [ ] Guía corta “un día de setup” enlazando KNOWLEDGE_GRAPH + INSTALL

### Fuera de alcance cercano

- Cloud multi-tenant / UI web
- Entity-linking con LLM
- Todos los lenguajes de programación
- Write-back SaaS por defecto

---

## Cómo priorizar

1. **P0** — rompe uso diario del agente → arreglar ya  
2. **P1** — diferencia omnicanal (filtros MCP, embeddings reales)  
3. **P2** — ergonomía / release  
4. **P3** — expansiones de conector  

Al cerrar un ítem: marcar aquí + línea en `CHANGELOG.md` + `just post` / `postcheck`.
