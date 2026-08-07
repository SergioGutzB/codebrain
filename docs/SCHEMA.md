# Schema versioning policy

## Current

- On-disk schema file: `schemas/v1.surql`
- Recorded in Surreal `meta` as `schema_version = "1"` (`codebrain_db::SCHEMA_VERSION`)
- `apply_schema` / `doctor --migrate` re-applies idempotent `DEFINE … IF NOT EXISTS` statements
- Relation tables include `resolves` (`symbol` → `document`) for Jira issue keys found in code

## Rules for changes

1. **Additive within v1** (new optional fields, new relation tables, new indexes): edit `schemas/v1.surql`, keep `SCHEMA_VERSION = "1"`, document in CHANGELOG. Existing DBs pick up DEFINEs on migrate.
2. **Breaking** (rename/drop fields, change id schemes, incompatible types): introduce `schemas/v2.surql`, bump `SCHEMA_VERSION` to `"2"`, and ship an explicit migration path (export/rebuild or transformative SurrealQL). Never silently mutate incompatible data.
3. **Embeddings meta** (`embedding_dimension` in `meta`) is independent of schema version; `doctor` validates it separately.

## Agent / release checklist

- [ ] Schema change described in CHANGELOG
- [ ] `migrate` test still idempotent
- [ ] `doctor` reports `schema_ok` after upgrade
- [ ] If breaking: migration notes + reindex instructions in INSTALL.md
