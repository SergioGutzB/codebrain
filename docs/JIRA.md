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
- Notion / Confluence connectors are still stubs.
