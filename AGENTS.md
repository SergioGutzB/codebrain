# Agent Contract — CodeBrain

> **Canonical entry point for every coding agent** (Cursor, Claude Code, OpenCode, Codex, Aider, etc.).
> If your tool only reads one file, read **this one**.

## Non-negotiables

1. Follow the **pre → implement → post** loop. Never skip validation.
2. Run repo scripts under `scripts/agent/` — do not invent ad-hoc checklists.
3. **Commits: no AI co-authors.** Never add `Co-Authored-By`, `Signed-off-by` for bots, or trailers naming Cursor/Claude/GPT/etc.
4. Prefer small, reviewable diffs. Match existing style. No drive-by refactors.
5. Do not commit secrets, `.env`, credentials, or local DB paths with data.

## Mandatory flow

```text
PRE  → ./scripts/agent/preflight.sh
IMP  → change code (respect docs/agent/CODING_STANDARDS.md)
POST → ./scripts/agent/postcheck.sh
COMMIT (only if user asks) → message rules in docs/agent/COMMIT_CONVENTION.md
           validated by ./scripts/agent/check-commit-msg.sh
```

Shortcut: `just pre` · `just validate` · `just post`

## Source of truth (read in order)

| Doc | Purpose |
|-----|---------|
| [docs/agent/PRE_IMPLEMENTATION.md](docs/agent/PRE_IMPLEMENTATION.md) | Checklist before coding |
| [docs/agent/CODING_STANDARDS.md](docs/agent/CODING_STANDARDS.md) | Rust / project style (errors, traits, concurrency, memory) |
| [docs/agent/RUST_PRACTICES.md](docs/agent/RUST_PRACTICES.md) | Concrete Rust examples for those rules |
| [docs/agent/TESTING.md](docs/agent/TESTING.md) | What/how to test |
| [docs/agent/POST_IMPLEMENTATION.md](docs/agent/POST_IMPLEMENTATION.md) | Checklist after coding |
| [docs/agent/COMMIT_CONVENTION.md](docs/agent/COMMIT_CONVENTION.md) | Commit messages (no co-author) |
| [docs/agent/VALIDATION.md](docs/agent/VALIDATION.md) | Gates & exit codes |
| [PLAN-IMPLEMENTACION.md](PLAN-IMPLEMENTACION.md) | Phased product plan |
| Architecture doc (repo root) | Design decisions |

## Tool adapters (thin)

| Agent | Adapter |
|-------|---------|
| Cursor | `.cursor/rules/`, `.cursor/skills/`, `.cursor/hooks.json` |
| Claude Code | `CLAUDE.md`, `.claude/settings.json` |
| OpenCode | `.opencode/AGENTS.md` → points here |
| Any other | This file + `scripts/agent/*` |

## Project map (quick)

- Workspace Rust: `crates/*`, schema `schemas/v1.surql`, CLI `codebrain`
- Config example: `codebrain.example.toml`
- Phase status: see `PLAN-IMPLEMENTACION.md` (v0.1 Foundation done)

## When unsure

1. Prefer **not** inventing APIs — mark as stub / Phase N.
2. Ask the user only for product decisions; engineering defaults live in the docs above.
3. After any non-trivial change, `just post` must pass before claiming done.
