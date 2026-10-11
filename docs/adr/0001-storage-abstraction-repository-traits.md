# Storage Abstraction via Dyn-Compatible Repository Traits

BaziFlowAgent previously coupled relational data access, disposable chat history caching, and HTML chart storage directly to `enum Database { Sqlite, D1 }`, a raw `SqlitePool`, and `Option<Box<s3::Bucket>>` inside `AppState`. We decided to decouple storage behind three segregated trait contracts: `UserRepository`, `ChatCacheRepository`, and `BlobStore`, dynamically dispatched via `Arc<dyn ...>` using `BoxFuture` return types.

## Status

accepted

## Considered Options

- **Option 1: Compile-time generic monomorphization** (`AppState<U: UserRepository, C: ChatCacheRepository, B: BlobStore>`). Rejected because generic parameters would infect all Axum handlers, Teloxide dispatcher trees, middleware signatures, and state extractors without measurable performance benefit on I/O boundaries.
- **Option 2: Unified God-trait `StorageBackend`**. Rejected because chat history cache is disposable with write-behind WAL semantics, user data is durable, and chart HTML is object/blob storage with presigning. Grouping them violates the Interface Segregation Principle.
- **Option 3: Segregated dyn-compatible traits (`Arc<dyn UserRepository>`, `Arc<dyn ChatCacheRepository>`, `Arc<dyn BlobStore>`)** [Chosen]. Matches existing `AppState.llm_service: Arc<dyn LlmService>` convention, permits independent backend mocking, and isolates Cloudflare D1/R2 from local SQLite/filesystem operation.

## Consequences

- All asynchronous trait methods return `futures::future::BoxFuture<'a, AppResult<T>>` to maintain Rust 2024 object safety without requiring generic trait methods or nightly features.
- Database transactions are method-scoped inside repository implementations (SQLite `pool.begin()` vs D1 `database.batch()`). Callers never manage raw transaction primitives.
- `AppState` constructor accepts trait implementations directly, decoupling configuration parsing from domain service initialization.
