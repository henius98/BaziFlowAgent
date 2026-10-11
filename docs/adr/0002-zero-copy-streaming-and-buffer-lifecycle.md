# Zero-Copy Streaming and Message Buffer Lifecycle

Streaming LLM completion tokens and managing consultation turns previously produced 1,000–1,500+ heap allocations per turn due to string cloning during stream forwarding, intermediate channel hops in `wrap_persisted_stream`, role prefix string formatting/parsing roundtrips, and $O(N)$ string shifts during context eviction. We decided to transition the streaming pipeline to `bytes::Bytes` and restructure `UserContext` into `VecDeque<ChatTurn>` using `Arc<str>` with incremental byte accounting.

## Status

accepted

## Considered Options

- **Option 1: Retain `String` payloads and apply micro-optimizations** (e.g. `String::with_capacity`). Rejected because sending `String` across channels requires owned transfers, and multi-consumer broadcasting or persistence tapping forces full heap cloning.
- **Option 2: Global zero-copy across all AST/JSON domains** (`serde` borrow lifetimes `&'a str` for Paipan models). Rejected because astrological models outlive the network frame lifecycle and cross thread/task boundaries, introducing complex lifetime parameters without significant latency benefits.
- **Option 3: Scoped Zero-Copy Pipeline (`bytes::Bytes` for transport + `Arc<str>` for context)** [Chosen]. Provides zero-allocation shallow cloning for SSE, WebSocket, and persistence taps while replacing $O(N^2)$ byte scans with $O(1)$ ring-buffer evictions.

## Consequences

- `LlmStream` transfers `mpsc::Receiver<bytes::Bytes>`. Delta tokens from `async-openai` transfer ownership into `Bytes` via $O(1)$ pointer transfer.
- `UserContext` replaces `Vec<String>` with `VecDeque<ChatTurn>`, storing structured `ChatRole` and `Arc<str>` content. Pushes and evictions run in $O(1)$ with incremental `total_bytes` tracking.
- Role tag prefix formatting (`format!("User: {}", ...)`) and parsing (`strip_prefix`) are eliminated across storage, memory, and prompt construction.
