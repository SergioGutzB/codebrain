# Conectar CodeBrain por MCP

CodeBrain expone su grafo (código + notas) a agentes LLM mediante un servidor MCP sobre **stdio**.

## Requisito previo

Indexa al menos una fuente antes de conectar; el servidor solo lee.

```bash
codebrain --config ./codebrain.toml index
```

## Arrancar el servidor

```bash
codebrain --config ./codebrain.toml serve
```

`stdout` es el canal del protocolo. Todos los logs salen por `stderr`, así que puedes
subir el detalle sin romper la sesión:

```bash
RUST_LOG=debug codebrain --config ./codebrain.toml serve
```

## Cursor

En `~/.cursor/mcp.json` (global) o `.cursor/mcp.json` (por proyecto):

```json
{
  "mcpServers": {
    "codebrain": {
      "command": "/ruta/absoluta/a/target/release/codebrain",
      "args": ["--config", "/ruta/absoluta/a/codebrain.toml", "serve"]
    }
  }
}
```

## Claude Code

```bash
claude mcp add codebrain -- /ruta/absoluta/a/codebrain --config /ruta/absoluta/a/codebrain.toml serve
```

Usa siempre rutas absolutas: el agente lanza el proceso con un directorio de trabajo distinto al tuyo.

## Tools disponibles

| Tool | Para qué sirve | Argumentos |
|------|----------------|------------|
| `list_sources` | Inventario de fuentes indexadas con conteos | `source_kinds?` |
| `search_symbols` | Buscar símbolos por nombre o FQN | `query`, `limit?`, `source_kinds?` |
| `explore_context` | Contexto omnicanal: símbolos + notas + aristas alrededor | `query`, `limit?`, `source_kinds?` |
| `semantic_search` | Búsqueda híbrida (embeddings + FTS + grafo) | `query`, `limit?`, `source_kinds?` |
| `graph_neighbors` | Expandir el grafo alrededor de un nodo | `node`, `depth?`, `limit?` |
| `add_architectural_decision` | Registrar ADR + aristas `about` (write-back vault opt-in) | `title`, `body`, `about[]?`, `write_vault?` |
| `promote_mention` | Promover `mentions` → `explains` tras revisión | `document`, `symbol` |

### Filtro `source_kinds`

Restringe resultados a canales o nombres de source configurados:

```json
{ "query": "plan", "source_kinds": ["jira", "confluence"] }
{ "source_kinds": ["git_repo"] }
{ "source_kinds": ["tickets"] }
```

Valores de kind: `git_repo`, `obsidian_vault`, `jira`, `confluence`, `notion` (también acepta el **nombre** de la source en TOML).

Mapa de módulos y aristas: [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md) · backlog: [`BACKLOG.md`](./BACKLOG.md).

### Tokens de nodo

`graph_neighbors` direcciona nodos con `kind:source:key`:

```text
symbol:code:Services::Greeter
file:code:services/greeter.rb
document:notes:Design.md
decision:system:Prefer Greeter facade
```

`explore_context` y `search_symbols` ya devuelven estos tokens, así que el flujo normal es
explorar primero y expandir después. Para preguntas en lenguaje natural (incluso cross-language),
usa `semantic_search`.

### ADR write-back

En `codebrain.toml`:

```toml
[adr]
write_vault = false
vault_source = "notes"
directory = "ADR"
created_by = "agent"
```

Con `write_vault = false` (default), `add_architectural_decision` **nunca** escribe en el vault.
Pasa `write_vault: true` en la llamada (o activa la config) para generar `ADR/<slug>.md`.

### Watch / reindex parcial

```toml
[index]
watch = true
debounce_ms = 750
```

Con `watch = true`, `codebrain serve` arranca un watcher en background. También puedes usar
`codebrain watch` como proceso dedicado.

### Transporte HTTP (equipos locales)

```toml
[mcp]
transport = "http"
bind = "127.0.0.1:8765"
allow_remote = false
```

Endpoint: `http://127.0.0.1:8765/mcp`. Los binds no-loopback se rechazan salvo `allow_remote = true`
(ver [`SECURITY.md`](./SECURITY.md)).

### Promover menciones

Cuando `explore_context` / `graph_neighbors` muestren un `mentions` correcto, confirma con:

```json
{
  "document": "document:notes:Design.md",
  "symbol": "symbol:code:Services::Greeter"
}
```

vía `promote_mention`. Con `linker.auto_promote_explains = true` el índice también crea `explains`
para menciones que ya pasan el umbral.

### Embeddings

En `codebrain.toml`:

```toml
[embeddings]
provider = "fastembed"   # none | fastembed | openai_compatible
model = "all-MiniLM-L6-v2"
dimension = 384
```

Con `provider = none`, `semantic_search` no falla: degrada a FTS + expansión de grafo.
Tras cambiar `provider`/`dimension`, vuelve a indexar y corre `codebrain doctor` (valida que la
dimensión del índice HNSW coincida).

### Relaciones que puedes recorrer

`defines`, `calls`, `imports` (código), `references` (wikilinks entre notas), `mentions`
(nota → símbolo, con `confidence`), `explains` (mención promovida), `about` (ADR → símbolo/archivo/nota)
y `resolves` (símbolo → ticket Jira).

## Resources

| URI | Contenido |
|-----|-----------|
| `codebrain://status` | Schema version + conteos por tabla |
| `codebrain://schema` | Leyenda de nodos/aristas (`kind:source:key`, `resolves`, `mentions`, …) |

`codebrain://schema` es el equivalente estructurado de [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md) para agentes.

## Límites de respuesta

Cada tool aplica un presupuesto (`QueryBudget`) para no desbordar la ventana de contexto:
40 nodos, 25 vecinos por nodo y profundidad máxima 2. Los valores pedidos se recortan a ese
rango en lugar de fallar, y la respuesta marca `truncated: true` cuando se llega al tope.

## Problemas frecuentes

| Síntoma | Causa probable |
|---------|----------------|
| El agente no lista tools | Ruta al binario incorrecta o no ejecutable |
| Todo devuelve vacío | Falta correr `codebrain index` |
| `graph_neighbors` da error de parámetros | El token no tiene forma `kind:source:key` |
