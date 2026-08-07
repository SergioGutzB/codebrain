# CodeBrain — security (threat model breve)

CodeBrain is **local-first**. It indexes paths you configure and serves them to LLM agents via MCP.

## Trust boundaries

| Surface | Risk | Mitigation |
|---------|------|------------|
| Source paths (`[sources.*].path`) | Reads any file under the tree (minus excludes) | Only point at repos/vaults you trust; review `index.exclude` |
| SurrealKV DB | Graph + note bodies on disk | Keep DB dir private (`~/.local/share/codebrain` by default); back up / encrypt disk as needed |
| MCP stdio | Agent process can call every tool | Run under your user; treat the agent as you |
| MCP HTTP | Network exposure of the graph | Default bind `127.0.0.1`; `allow_remote=false` refuses non-loopback |
| ADR write-back | Writes Markdown into the vault | `adr.write_vault=false` by default; tool arg must opt in |
| Embeddings HTTP | API keys / note text leave the machine | Prefer `fastembed` / `none` offline; use `api_key_env` (never commit keys) |

## Secrets

- Never put API keys in `codebrain.toml`. Use env vars referenced by `embeddings.api_key_env`.
- Do not commit personal `codebrain.toml` with absolute home paths into public repos if that leaks identity (optional).
- Agent commits: no AI co-author trailers (see `docs/agent/COMMIT_CONVENTION.md`).

## MCP HTTP hardening checklist

1. Keep `bind` on loopback unless you intentionally share on a LAN.
2. Set `allow_remote = true` only with OS firewall / VPN controls.
3. Prefer stdio for single-user Cursor / Claude Code sessions.
4. Stop the process when unused (`Ctrl+C` / service stop).

## What CodeBrain does **not** do (yet)

- AuthN/AuthZ between MCP clients (HTTP is open to anyone who can reach the port).
- Sandboxing of tool side-effects beyond config flags (`write_vault`, path excludes).
- Encryption of the embedded DB at rest (rely on OS disk encryption).
