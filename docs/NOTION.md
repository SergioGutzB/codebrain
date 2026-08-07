# CodeBrain — Notion connector (Phase 7)

Read-only ingestion of Notion pages shared with an **internal integration** as `document` nodes.

## Setup

1. Create an integration: https://www.notion.so/my-integrations  
2. Copy the **Internal Integration Secret**.  
3. Share target pages/databases with the integration (⋯ → Connections).  
4. Export:

```bash
export NOTION_TOKEN=secret_...
# alias also accepted: NOTION_API_KEY
```

5. Config:

```toml
[sources.notion]
kind = "notion"
# optional title search filter:
# query = "architecture"
max_issues = 50
# token_env = "NOTION_TOKEN"  # only if not using the default env names
```

6. Index:

```bash
codebrain --config ./codebrain.toml index --source notion
```

## Graph shape

| Node / edge | Meaning |
|-------------|---------|
| `document:notion:<pageId>` | Page body (blocks → text) |
| `mentions` | Page body cites a code symbol |
| `references` | Page cites an indexed Jira issue key → ticket |

## Limits

- Only pages **shared** with the integration appear in search.
- Nested blocks depth ≤ 3; skips diving into child pages (they are separate search hits).
- Read-only (no write-back).
