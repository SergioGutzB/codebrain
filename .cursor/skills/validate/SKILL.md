---
name: validate
description: >-
  Runs CodeBrain quality gates (fmt, clippy -D warnings, workspace tests) via
  scripts/agent/validate.sh. Use when checking health, before PRs, after
  refactors, or when the user asks to validate/lint/test the project.
---

# Validate

## Instructions

```bash
./scripts/agent/validate.sh
# equivalent: just validate
```

See [docs/agent/VALIDATION.md](../../../docs/agent/VALIDATION.md) for exit codes.

If red: fix the first failing gate, re-run, repeat. Do not ignore clippy warnings.
