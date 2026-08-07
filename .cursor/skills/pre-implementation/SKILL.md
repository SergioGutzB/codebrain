---
name: pre-implementation
description: >-
  Runs the CodeBrain pre-implementation workflow: orient on AGENTS.md and plan,
  execute preflight.sh, confirm scope/tests/risks before coding. Use before
  implementing features, fixes, refactors, or starting a new phase.
---

# Pre-implementation

## Instructions

1. Read [AGENTS.md](../../../AGENTS.md) and [docs/agent/PRE_IMPLEMENTATION.md](../../../docs/agent/PRE_IMPLEMENTATION.md).
2. Run:

```bash
./scripts/agent/preflight.sh
```

3. If preflight fails, fix the environment/workspace before any feature work.
4. State briefly: problem, approach, out of scope, risks, tests.
5. Confirm the work matches the current phase in `PLAN-IMPLEMENTACION.md`.
6. Only then start editing code.

## Do not

- Skip preflight because “it’s a small change” (still run it for compile health)
- Expand into later phases without user approval
