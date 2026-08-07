# Agent docs index

| Doc | Role |
|-----|------|
| [../AGENTS.md](../AGENTS.md) | Canonical contract (all agents) |
| [PRE_IMPLEMENTATION.md](PRE_IMPLEMENTATION.md) | Before coding |
| [POST_IMPLEMENTATION.md](POST_IMPLEMENTATION.md) | After coding |
| [CODING_STANDARDS.md](CODING_STANDARDS.md) | Rust / crate rules (errors, traits, async, memory) |
| [RUST_PRACTICES.md](RUST_PRACTICES.md) | Examples for the standards above |
| [TESTING.md](TESTING.md) | Test expectations |
| [COMMIT_CONVENTION.md](COMMIT_CONVENTION.md) | Commits without AI co-authors |
| [VALIDATION.md](VALIDATION.md) | Scripts & exit codes |

## Scripts

```bash
./scripts/agent/preflight.sh
./scripts/agent/validate.sh
./scripts/agent/postcheck.sh
./scripts/agent/check-commit-msg.sh
./scripts/agent/install-hooks.sh
```
