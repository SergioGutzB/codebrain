# Coding standards — CodeBrain (Rust)

Canonical Rust rules for every agent. Deep examples: [RUST_PRACTICES.md](./RUST_PRACTICES.md).

## Style

- Edition **2024**, `rust-version` from workspace
- `cargo fmt` is law — never hand-format against `rustfmt.toml`
- Clippy: **deny warnings** + panic/unwrap family (see `clippy.toml` / `validate.sh`)
- Prefer clarity over cleverness; no drive-by renames

## Crate boundaries

| Crate | May depend on | Must not |
|-------|---------------|----------|
| `codebrain-cli` | core, db, mcp | tree-sitter / heavy parsers directly |
| `codebrain-core` | db, connector | MCP transport details |
| `codebrain-db` | surrealdb | connectors, clap |
| `codebrain-connector*` | connector trait crate | db writes (orchestrator owns persistence) |
| `codebrain-mcp` | core | direct Surreal client (go through core/db) |

## Crates & modularity (mandatory)

- Every subsystem with an independent responsibility is a **workspace crate**, not a large module
  hidden inside the CLI or core.
- A crate must have one reason to change, a minimal public API, its own unit tests, and no cyclic
  dependencies.
- Inside a crate, split modules by capability (`discovery`, `extract`, `language`, `persistence`);
  avoid `lib.rs` / `main.rs` files larger than roughly 300 lines.
- Keep implementation modules private by default. Re-export only stable contracts from `lib.rs`.
- Put shared domain types in the lowest dependency crate that owns the concept; never duplicate
  DTOs between crates.
- Depend on traits at boundaries and inject implementations. Tests must be able to replace I/O,
  storage, clocks, or external services without global state.
- Each crate must pass independently:

```bash
cargo test -p <crate>
cargo clippy -p <crate> --all-targets -- -D warnings
```

- New crates require: purpose in `Cargo.toml`, owner boundary in this table, tests, and inclusion in
  the workspace validation.
- Do not create generic `utils`, `common`, or “god” crates. Name crates/modules after domain
  capabilities.

## Error handling (no panic in libs)

| Layer | Pattern |
|-------|---------|
| Library (`*-db`, `*-connector`, `*-core`) | `thiserror` enums + `Result<T, E>` |
| Binary / orchestration edge | `anyhow::Result` + `context` / `with_context` |
| Logging | `tracing` only — never `println!` in libs |

**Forbidden in non-test production code:**

- `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`
- `assert!` for control flow (tests only)
- Indexing that can panic (`slice[i]`) when `get` / `get_mut` works
- `lock().unwrap()` on mutexes — use map/`poison` handling or `parking_lot` carefully

**Allowed:**

- `?` everywhere fallible work propagates
- `Option` for absence; do not overload `Result` for “not found” when absence is normal
- Convert foreign errors with `#[from]` / `map_err` — preserve cause chain
- In tests: `unwrap`/`expect` OK (`allow-*-in-tests` in clippy)

Stubs for future phases: `anyhow::bail!("… Phase N")` or typed `Error::NotImplemented` — never `todo!()`.

## Traits & abstraction

- Behavior variation → **trait** (`Connector`, future `Embedder`, `QueryService`)
- Prefer small traits (ISP): one capability per trait when possible
- Bound generics explicitly (`T: Connector + Send + Sync`)
- Use `async_trait` only on public async traits until RPITIT is adopted project-wide
- Do not invent traits for a single unused implementor — wait for the second caller
- Dynamic dispatch (`dyn Trait`) only at boundaries that need runtime plugin switching; default to static dispatch / generics in hot paths

## Concurrency & async

- **Tokio** is the only async runtime
- Never block the runtime: no sync `std::fs` / heavy CPU in `async fn` without `tokio::fs` or `spawn_blocking`
- Share state with `Arc<T>`; interior mutability via `tokio::sync` (`Mutex`, `RwLock`, `Semaphore`) — not `std::sync::Mutex` across `.await`
- Bound concurrency with `Semaphore` / buffered streams for indexing jobs
- Channels (`mpsc`) for work queues (watcher → indexer); avoid unbounded growth
- Cancellation: honor `CancellationToken` / drop of receivers in long jobs (Phase 5+)
- `Send + Sync` on types stored in MCP/server state

## Memory & performance

- Prefer borrowing (`&str`, `&[u8]`, `Path`) at API boundaries; allocate (`String`, `PathBuf`, `Vec`) at ownership edges
- Avoid clones in hot loops — clone only when ownership must fork
- Use `Bytes` / `Arc<str>` for large shared immutable payloads when cloning would dominate
- Stream / batch I/O (index `batch_size`); do not load entire repos into memory
- Pre-allocate with `Vec::with_capacity` when length is known
- Prefer iterators / `try_fold` over temporary `Vec` when a single pass suffices
- Zeroize or drop secrets promptly; never log tokens/paths that embed credentials
- Measure before micro-optimizing; keep indexing path allocation-conscious by design

## Surreal / schema

- Schema in `schemas/v*.surql`; applied via `codebrain-db` migrate
- Migrations **idempotent** (`DEFINE … IF NOT EXISTS`)
- Bump `SCHEMA_VERSION` on incompatible changes

## Config

- Defaults in `Config::default()`, overrides via TOML + `CODEBRAIN_*`
- Paths: `expand_path`; examples use `~/…`

## Comments

- Comment **why**, not what
- Phase stubs: one-line bail is enough
