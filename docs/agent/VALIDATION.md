# Validation gates

Machine-checkable quality bar for CodeBrain. All agents must use these scripts.

## Commands

| Script | When | Exit 0 means |
|--------|------|--------------|
| `scripts/agent/preflight.sh` | Before implementation | Toolchain + workspace compile |
| `scripts/agent/validate.sh` | Anytime / CI | fmt + clippy + tests |
| `scripts/agent/postcheck.sh` | After implementation | validate + diff hygiene hints |
| `scripts/agent/check-commit-msg.sh` | Before commit | message convention OK |

Just aliases: `just pre` · `just validate` · `just post`

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Pass |
| 1 | Generic failure (compile/test/lint) |
| 2 | Commit message policy violation |
| 3 | Missing required tool (rustup/cargo) |

## CI parity

GitHub Actions (`.github/workflows/ci.yml`) runs the same `validate.sh` on push/PR.

## Agent rule

If validation fails: **fix, re-run, only then continue**. Do not mark the task complete on a red gate.
