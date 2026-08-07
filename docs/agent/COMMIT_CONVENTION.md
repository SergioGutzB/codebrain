# Commit convention

## Format (Conventional Commits)

```text
<type>(<scope>)?: <summary>

[optional body]

[optional footer]
```

### Types

| Type | Use |
|------|-----|
| `feat` | User-facing capability |
| `fix` | Bug fix |
| `refactor` | No behavior change |
| `test` | Tests only |
| `docs` | Documentation only |
| `chore` | Tooling, deps, meta |
| `perf` | Performance |
| `build` | Build system / CI |

### Scopes (suggested)

`cli`, `core`, `db`, `connector`, `mcp`, `schema`, `agent`, `docs`

### Summary

- Imperative, lowercase, no period: `add surreal migrate idempotency`
- ≤ 72 chars subject line

## Forbidden trailers (hard fail)

Agents and humans **must not** add:

- `Co-Authored-By:`
- `Co-authored-by:`
- `Signed-off-by:` for AI/bots (`cursor`, `claude`, `gpt`, `copilot`, `opencode`, …)
- `Generated-by:` / `Assisted-by:` AI banners

Rationale: commits are authored by the human owner of the repo; tools are not co-authors.

## Allowed footers

- `Fixes: #123`
- `Refs: PLAN Phase 1`
- `BREAKING CHANGE: …`

## Validation

```bash
./scripts/agent/check-commit-msg.sh .git/COMMIT_EDITMSG
# or pipe:
echo "feat(db): add schema v1" | ./scripts/agent/check-commit-msg.sh
```

Git hook (after `./scripts/agent/install-hooks.sh`):

- `commit-msg` → runs the checker
- blocks commit if co-author / invalid type

## Examples

```text
feat(db): apply idempotent schema v1

Wire SurrealKV open + migrate on `codebrain init`.
```

```text
chore(agent): add preflight and postcheck scripts
```

```text
fix(cli): surface doctor failures with non-zero exit
```

## Signing (optional GPG/SSH)

If the maintainer enables `commit.gpgsign` / SSH signing locally, agents must **not** disable it (`--no-gpg-sign`) unless the user explicitly requests it.
