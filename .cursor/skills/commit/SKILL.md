---
name: commit
description: >-
  Creates git commits for CodeBrain using Conventional Commits and forbids AI
  co-author trailers (no Co-Authored-By). Use only when the user explicitly asks
  to commit; validates the message with check-commit-msg.sh.
---

# Commit (no AI co-authors)

## Instructions

1. Confirm the user **explicitly** asked to commit.
2. Run `./scripts/agent/postcheck.sh` first if not already green.
3. Draft message per [docs/agent/COMMIT_CONVENTION.md](../../../docs/agent/COMMIT_CONVENTION.md).
4. Validate:

```bash
echo "type(scope): summary" | ./scripts/agent/check-commit-msg.sh
```

5. Stage relevant files only (never secrets).
6. Commit with HEREDOC; **do not** add `Co-Authored-By` or AI trailers.
7. Do not use `--no-verify` or `--no-gpg-sign` unless the user requests it.
8. Run `git status` after commit.

## Forbidden

```text
Co-Authored-By: Cursor <...>
Co-Authored-By: Claude <...>
Generated-By: ...
```

## Example

```bash
git commit -m "$(cat <<'EOF'
feat(db): apply idempotent schema v1

EOF
)"
```
