# CodeBrain — Confluence connector (Phase 7)

Read-only ingestion of Confluence Cloud pages as `document` nodes. Uses the same Atlassian API token as Jira.

## Setup

```bash
export JIRA_BASE_URL=https://your.atlassian.net
export JIRA_EMAIL=you@company.com
export JIRA_API_TOKEN=...
```

```toml
[sources.wiki]
kind = "confluence"
cql = "type = page AND space = \"ENG\" ORDER BY lastmodified DESC"
max_issues = 50   # max pages per run
```

```bash
codebrain --config ./codebrain.toml index --source wiki
```

## Graph shape

| Node / edge | Meaning |
|-------------|---------|
| `document:wiki:<pageId>` | Page body (ADF preferred, HTML storage fallback) |
| `mentions` | Page body cites a code symbol name/FQN |
| `references` | Page cites a Jira issue key already indexed → ticket document |

## Limits

- Read-only (no write-back).
- Rate limits: respects `Retry-After`; small delay between pages.
- Notion connector is still pending.
