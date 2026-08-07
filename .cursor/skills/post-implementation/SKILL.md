---
name: post-implementation
description: >-
  Runs the CodeBrain post-implementation workflow: postcheck.sh (fmt, clippy,
  tests), diff hygiene, and handoff summary. Use after finishing code changes
  and before claiming a task is done or creating a commit/PR.
---

# Post-implementation

## Instructions

1. Read [docs/agent/POST_IMPLEMENTATION.md](../../../docs/agent/POST_IMPLEMENTATION.md).
2. Run:

```bash
./scripts/agent/postcheck.sh
```

3. On failure: fix issues and re-run until exit `0`.
4. Verify docs/example config if CLI, schema, or config keys changed.
5. Handoff to the user: what changed, validation result, follow-ups.
6. **Do not commit** unless the user explicitly asked. If committing, use the `commit` skill.

## Definition of Done

- `postcheck.sh` exit 0
- Tests cover the change
- No AI co-author trailers anywhere in the commit message draft
