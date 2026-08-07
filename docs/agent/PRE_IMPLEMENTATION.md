# Pre-implementation checklist

Run **before** writing or editing production code.

## 1. Orient

- [ ] Read `AGENTS.md`
- [ ] Identify phase / milestone in `PLAN-IMPLEMENTACION.md`
- [ ] Confirm the change is **in scope** for the current phase (no SaaS/MCP if still on Phase 1, etc.)
- [ ] List files/crates likely touched

## 2. Preflight (mandatory)

```bash
./scripts/agent/preflight.sh
# or: just pre
```

Must exit `0`. If it fails, fix environment issues first.

## 3. Design gates

Answer briefly (in the PR/commit body or chat):

1. **Problem** — what user/system need?
2. **Approach** — smallest change that works?
3. **Out of scope** — what we explicitly will not do?
4. **Risks** — schema, perf, security, breaking CLI?
5. **Tests** — which new/updated tests prove it?

## 4. Constraints

- Local-first; no secrets in the graph or git
- Grafo before embeddings (architecture invariant)
- Connector changes go through `codebrain-connector` types
- Schema changes = version bump plan in `schemas/` + migrate idempotent

## 5. Ready to implement when

- Preflight green
- Scope clear
- Test strategy named
- No open blocker questions that would reverse the design
