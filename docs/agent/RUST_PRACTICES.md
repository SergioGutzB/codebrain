# Rust practices — examples (CodeBrain)

Companion to [CODING_STANDARDS.md](./CODING_STANDARDS.md). Agents: follow the standards doc; use this when unsure how to apply a rule.

## Errors

```rust
// ❌ BAD — panic in library
let db = open_embedded(path).await.unwrap();

// ✅ GOOD — typed / propagated
let db = open_embedded(path)
    .await
    .map_err(DbError::from)?;

// ✅ GOOD — edge with context
use anyhow::Context;
let cfg = load_config(Some(path)).context("load codebrain.toml")?;
```

```rust
// ❌ BAD — todo in shipped stub
todo!("implement later");

// ✅ GOOD — explicit failure
bail!("code connector is not implemented yet (Phase 1)");
```

Absence vs failure:

```rust
// Not found is normal → Option
fn find_symbol(&self, fqn: &str) -> Option<&SymbolNode> { ... }

// I/O / DB failure → Result
async fn load_symbol(&self, fqn: &str) -> Result<SymbolNode, DbError> { ... }
```

## Traits

```rust
#[async_trait]
pub trait Connector: Send + Sync {
    fn id(&self) -> &str;
    fn source_kind(&self) -> SourceKind;
    async fn discover(&self, ctx: &IndexContext) -> anyhow::Result<Vec<WorkItem>>;
    async fn extract(&self, item: &WorkItem) -> anyhow::Result<ExtractBatch>;
}

// Orchestrator depends on the trait, not concrete crates:
async fn run_index(c: &dyn Connector, ctx: &IndexContext) -> anyhow::Result<()> { ... }
```

Prefer generics on hot paths:

```rust
async fn run_index_static<C: Connector>(c: &C, ctx: &IndexContext) -> anyhow::Result<()> { ... }
```

## Async / concurrency

```rust
// ❌ BAD — blocking FS on async worker
async fn read_note(path: &Path) -> anyhow::Result<String> {
    Ok(std::fs::read_to_string(path)?) // blocks runtime
}

// ✅ GOOD
async fn read_note(path: &Path) -> anyhow::Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

// ✅ GOOD — CPU-heavy parse off runtime
let batch = tokio::task::spawn_blocking(move || extract_tree_sitter(bytes)).await??;
```

```rust
// Bound parallel file work
let sem = Arc::new(Semaphore::new(num_cpus));
for item in items {
    let permit = sem.clone().acquire_owned().await?;
    tokio::spawn(async move {
        let _permit = permit;
        // ...
    });
}
```

Never hold `std::sync::Mutex` across `.await`. Use `tokio::sync::Mutex` if you must lock in async.

## Memory

```rust
// ❌ BAD — unnecessary clone in loop
for s in symbols {
    index.insert(s.fqn.clone(), s.clone());
}

// ✅ GOOD — move / borrow
for s in symbols {
    index.insert(s.fqn, s); // move
}

// ✅ GOOD — API borrows
fn resolve_wikilink<'a>(title: &str, notes: &'a [DocumentNode]) -> Option<&'a DocumentNode> { ... }
```

```rust
// Batch instead of one giant allocation
for chunk in files.chunks(config.index.batch_size) {
    persist_batch(chunk).await?;
}
```

## Clippy enforcement

`scripts/agent/validate.sh` denies (among warnings):

- `clippy::unwrap_used`, `clippy::expect_used`
- `clippy::panic`, `clippy::todo`, `clippy::unimplemented`
- `clippy::unreachable`

Tests may still `unwrap`/`expect` (`allow-*-in-tests` in `clippy.toml`).
