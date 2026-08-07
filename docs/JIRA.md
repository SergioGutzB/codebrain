# CodeBrain — Jira connector (Phase 7)

Read-only ingestion of Jira Cloud issues as `document` nodes, with `RESOLVES` edges from code symbols when issue keys appear in source files.

## Setup

1. Create an Atlassian API token: https://id.atlassian.com/manage-profile/security/api-tokens
2. Export (never commit):

```bash
export JIRA_BASE_URL=https://your.atlassian.net
export JIRA_EMAIL=you@company.com
export JIRA_API_TOKEN=...
```

Optional: `source ~/monokera/jira-tui/.env` if you already keep tokens there.

3. Config:

```toml
[sources.tickets]
kind = "jira"
jql = "assignee = currentUser() AND updated >= -30d ORDER BY updated DESC"
max_issues = 100
# path = "https://your.atlassian.net"  # optional override of JIRA_BASE_URL
```

4. Index:

```bash
codebrain --config ./codebrain.toml index --source tickets
# or full:
codebrain --config ./codebrain.toml index
```

## Incremental sync (`updated` cursor)

After the first successful pull, CodeBrain stores a high-water mark in the DB:

| Meta key | Value |
|----------|--------|
| `jira.<source>.updated_cursor` | RFC3339 timestamp of the newest `updated` seen |

Later runs rewrite the JQL to:

```text
(<your JQL without ORDER BY>) AND updated >= "YYYY/MM/DD HH:MM" ORDER BY updated ASC
```

Jira interprets bare `"YYYY/MM/DD HH:MM"` in the user/site timezone; CodeBrain formats the stored UTC cursor in **local time** before building JQL (content-hash skip still dedupes the boundary minute).

| Mode | Behaviour |
|------|-----------|
| First run / no cursor | Full configured JQL; may prune issues absent from the page |
| Incremental | Delta JQL; **does not** delete tickets missing from the page |
| `codebrain index --force` | Ignores cursor (full JQL); refreshes cursor afterward |

Deleted Jira issues are only pruned on a full sync (`--force` or first run). Incremental mode prioritizes cheap re-index over tombstones.

## Graph shape

| Node / edge | Meaning |
|-------------|---------|
| `document:tickets:MM-147` | Issue body (summary + ADF description flattened) |
| `symbol:backend:…` → `resolves` → ticket | Issue key found in that file’s source text |

Keys matched: `\b[A-Z][A-Z0-9]+-\d+\b` (e.g. `MM-147`, `SALES-12`).

## Limits

- Read-only (no write-back to Jira).
- Rate limits: respects `Retry-After`; small delay between pages.
- `max_issues` caps each run (default 100).
