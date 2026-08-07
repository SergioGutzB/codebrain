# Testing standards

## Pyramid

1. **Unit** — parsers, config, pure transforms (fast, no DB)
2. **Integration** — Surreal **memory** engine + schema (`codebrain-db`)
3. **CLI smoke** — `init` / `doctor` / `status` against temp dirs (scripts or `#[tokio::test]`)
4. **Manual E2E** — Cursor/MCP only when Phase 3+ lands

## Rules

- Every bugfix gets a regression test when feasible
- Schema: `migrate_is_idempotent` (or successor) must stay green
- Prefer fixtures under `testdata/` (Phase 1+) over inline giant strings
- Do not require network in default `cargo test`
- Do not write into the developer’s real `~/.local/share/codebrain` from tests — use `tempfile`

## Naming

```rust
#[tokio::test]
async fn migrate_is_idempotent() { ... }

#[test]
fn expand_home_strips_tilde() { ... }
```

## Coverage expectations by change type

| Change | Minimum tests |
|--------|----------------|
| Parser / linker | unit + fixture |
| Schema / migrate | idempotent integrate |
| CLI flag / config key | unit deserialize + doctor path |
| Connector extract | fixture → `ExtractBatch` asserts |
| MCP tool | contract/smoke (Phase 3) |

## Running

```bash
./scripts/agent/validate.sh
# or: just validate
# or: cargo test --workspace
```
