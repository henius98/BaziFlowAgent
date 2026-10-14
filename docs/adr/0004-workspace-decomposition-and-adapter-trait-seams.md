# Workspace Decomposition and Adapter Trait Seams

Astrological chart computation, solar time calculations, and WuXing cycle evaluation were previously tightly coupled with Tokio runtime, Teloxide Telegram bot primitives, SQLite connection pools, and network HTTP client dependencies in a monolithic crate. External network dependencies (Almanac HTTP endpoints, Paipan calculation APIs, OpenAI LLM services) lacked trait boundaries, hindering deterministic unit testing. We decided to decompose the codebase into a Cargo workspace with an isolated pure domain crate (`baziflow-core`) and introduce segregation trait seams for external I/O adapters in `baziflow-agent`.

## Status

accepted

## Considered Options

- **Option 1: Retain monolithic crate and mock network via HTTP servers** (`mockito`). Rejected because integration tests required spinning up network listener mock servers, slowing test execution, and pure astrological domain math remained entangled with async runtime and storage dependencies.
- **Option 2: Fine-grained multi-crate explosion** (separate crates for `baziflow-astrology`, `baziflow-models`, `baziflow-solar`, `baziflow-telegram`, `baziflow-api`, `baziflow-storage`). Rejected because excessive crate boundaries introduce build graph serialization, maintenance churn, and public API friction across tightly coupled application features without added safety.
- **Option 3: Two-crate Cargo workspace + Adapter trait seams** [Chosen]. Isolates all pure domain and astrological math into `crates/baziflow-core` with minimal dependencies (`chrono`, `serde`, `thiserror`), while introducing clean trait boundaries (`AlmanacProvider`, `ChartProvider`, `LlmService`, `BlobStore`) in `baziflow-agent` with backward-compatible facades.

## Consequences

- `crates/baziflow-core` has zero database, network client, or Telegram dependencies, executing standalone unit tests in sub-second time.
- `baziflow-agent` re-exports core astrological models and calculations via backward-compatible module facades, preventing breaking changes across existing handlers and tests.
- External network I/O is segregated behind traits (`AlmanacProvider`, `ChartProvider`, `LlmService`), allowing 100% in-memory mocking and fast deterministic unit/integration testing without network sockets.
- Workspace dependencies (`chrono`, `serde`, `serde_json`, `thiserror`) are centralized under `[workspace.dependencies]` to prevent version drift.
