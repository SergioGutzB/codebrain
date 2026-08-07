# Post-implementation checklist

Run **after** code changes, **before** claiming the task is done.

## 1. Postcheck (mandatory)

```bash
./scripts/agent/postcheck.sh
# or: just post
```

Must exit `0`. Includes fmt, clippy, tests, and commit-trailer scan on staged/unstaged diffs when git exists.

## 2. Functional verification

- [ ] Happy path exercised (CLI or unit/integration test)
- [ ] Error paths return typed/`anyhow` errors — no silent `unwrap` in libs
- [ ] Stubs still return clear “Phase N” messages if unfinished
- [ ] Docs updated if CLI flags, config keys, or schema changed

## 3. Diff hygiene

- [ ] No unrelated refactors
- [ ] No leftover `TODO` that should be tickets / phase notes
- [ ] No secrets, absolute personal paths committed in examples without `~`
- [ ] `codebrain.example.toml` / README updated when needed

## 4. Definition of Done

| Gate | Command / proof |
|------|-----------------|
| Format | `cargo fmt --check` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Tests | `cargo test --workspace` |
| Schema (if touched) | migrate idempotent test still passes |
| Agent contract | no `Co-Authored-By` in commit message draft |

## 5. Handoff

Report to the user:

1. What changed (crates/files)
2. How it was validated (`just post` result)
3. What remains (next phase / follow-ups)
4. **Do not commit** unless the user explicitly asked
