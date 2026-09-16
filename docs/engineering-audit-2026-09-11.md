# Rust service engineering audit — 2026-09-11

## Executive summary

The application was already separated into Telegram adapters, shared services, SQLite repositories, and an HTTP/SSE/WebSocket API. That structure was retained. The main problems were missing admission/lifecycle controls, locks held across Telegram awaits, prematurely released processing guards, a largely unconstrained legacy WebSocket, and correctness/security faults in persistence and chart delivery.

Implemented a versioned, owner-scoped persistent gateway at `/api/v1/ws`, bounded connection/event/work resources, shared Telegram throttling, cancellation-aware LLM streaming, coordinated shutdown, safer chart capabilities/HTML, and focused database/concurrency fixes. Existing application functionality and the legacy fortune protocol remain available, with documented security-related changes.

**Measured:** chart decoding is approximately 51% faster in the repeated parser microbenchmark. The new gateway's buffer tuning reduced combined load-generator/server RSS at 1,000 connections from about 168 MiB to 45–53 MiB. Final repeated throughput was 33,186–34,068 state requests/second. These are short local measurements, not production capacity guarantees or measurements of Telegram/LLM throughput.

**Verification:** 17 functional tests passed; the two opt-in performance tests passed in multiple release runs. All-target checking, strict all-feature Clippy, formatting, and the release build passed. All six Mermaid sequences passed the actual Mermaid parser in a local browser. Nothing was committed or deployed. Existing staged changes were preserved.

## Discovery and architecture

- One Rust 2024 package (`baziflow-agent`), library and binary, no Cargo workspace. No declared MSRV; tested with `rustc 1.98.1`. A lower supported compiler version was not established.
- Tokio multithread runtime; teloxide 0.17 dispatcher uses **long polling**, not Telegram webhooks or MTProto. Commands, callback queries, and free text are routed through dptree. There are no implemented general media/file/inline-query integration capabilities.
- Axum 0.8 handles HTTP, SSE, the legacy fortune WebSocket, and the new gateway. Authentication uses hashed random owner keys from SQLite.
- Primary SQLite retains profiles, preferences, credentials, and LLM metadata. A separate disposable WAL database retains bounded recent chat history through a batched writer. Existing indexes were retained.
- Shared `Arc<AppState>`, DashMap user contexts, and a process singleton remain. Brief synchronous locks protect the bounded identity-rate table; no such lock spans an await. Owner-specific event maps avoid scanning unrelated clients during fan-out.
- External services: chart/supplement APIs, MingDecode almanac, OpenAI-compatible chat completions, optional R2 storage. The service layer contains no Telegram transport types. Trusted endpoint injection makes integration tests contact local mocks.
- Deployment remains systemd on Raspberry Pi/DietPi with the existing ARM cross-build workflow. No container manifest or original benchmark/profiling harness was found. Added a separate quality-check workflow and repository-local formatting configuration.
- Cargo lockfile package count changed from 452 to 450. Chat-completion-only features removed a duplicate tungstenite stack; Telegram throttling added `vecrem`; `tokio-util` became an explicit dependency. Existing native TLS/Rustls and reqwest version overlap remains due to upstream crate dependencies. No indiscriminate upgrades were performed.
- Release settings remain thin LTO, one codegen unit, size optimization, stripping, and abort-on-panic. Their ARM build/size tradeoffs were retained; no CPU-throughput claim is made for those flags.

Final boundaries:

| Layer | Responsibility |
| --- | --- |
| Telegram | Update routing, private API-key delivery, keyboards, drafts, shared throttled sends |
| HTTP/WS/SSE | Authentication, admission, input/protocol validation, deadlines, transport output |
| Application core | Owner-scoped model/state actions and existing Bazi/almanac/chat orchestration |
| Event hub | Ephemeral owner notifications; bounded nonblocking fan-out |
| Persistence | Parameterized SQLite operations, atomic log IDs, authoritative migration history |
| External clients | Reused HTTP pools; bounded chart/almanac responses; bounded LLM producers |

See [project sequences](project-flow-sequence.md) and [the complete protocol](websocket-integration.md).

## Critical findings and resolutions

| Priority | Location | Root cause / impact | Evidence and resolution |
| --- | --- | --- | --- |
| P0 | `bot/messages.rs`, `bot/callbacks.rs`, `models/processing_guard.rs` | DashMap guards crossed network awaits; spawned work outlived the guard that should exclude concurrent operations. Deadlock/stale-state risk. | Static control-flow inspection. Acquisition now completes before I/O, is shared across transports, and moves into tracked work. Cancellation/permit-release regression passes. |
| P0 | `repos/mod.rs` | Migration errors caused deletion of migration history and an automatic retry. Incorrect migrations could be concealed/reapplied. | Destructive fallback removed; comments updated. Startup now fails closed while preserving history. |
| P0 | `services/paipan/bazi_utils.rs` | Subtracting one from the first luck-cycle index underflowed. Release abort-on-panic made this operationally severe. | Added first-cycle regression; use saturating subtraction. |
| P1 | `api/handlers.rs`, `api/gateway.rs`, `api/admission.rs` | Missing connection/frame/lifetime controls, rate limiting, and streaming admission. Slow or abusive clients could exhaust resources. | Shared semaphores, per-owner limits, strict typed parsing, bounded queues, send/idle deadlines, and rotation checks. Socket tests cover malformed/oversized input, authorization, revocation, limits, heartbeat, and shutdown. |
| P1 | `bot/commands.rs`, `bot/callbacks.rs` | API-key generation/regeneration could disclose keys in a group. | Key management now requires the owner's private chat. Creation atomically ensures the owner row exists before inserting a credential. |
| P1 | `services/bazi_service.rs`, `api/charts.rs`, `paipan/formatter.rs` | Predictable public filenames exposed personal charts; external text was inserted as HTML. | New 256-bit capability tokens, no direct directory serving, escaped external text, restrictive response headers, and XSS regression. Existing local links require regeneration. |
| P1 | `services/llm.rs`, `models/common.rs`, API/bot consumers | A failed producer looked like successful EOF; partial output could be saved and reported as complete. | Explicit producer completion acknowledgement. Cancelled/failed streams report interruption; malformed upstream SSE regression passes. |
| P1 | `main.rs`, `scheduler.rs`, cache writer | Runtime exit dropped unrelated tasks, connections, and queued persistence. | SIGINT/SIGTERM/service-exit coordination, tracked work, socket cancellation, bounded drain, cache flush barrier, and pool closure. Deadline expiry remains explicitly lossy. |
| P1 | `models/state.rs`, `repos/chat_history_cache.rs` | History was bounded by count only; cache queue/page-cache settings permitted high memory consumption. | Context byte/count admission, bounded cache turn size/queue, smaller SQLite page caches, acknowledged clears. These are configured upper bounds, not heap measurements. |
| P2 | `repos/mod.rs` | Random timestamp-derived log IDs could collide during bursts. | SQLite allocates INTEGER PRIMARY KEY values atomically. |
| P2 | `services/paipan/client.rs` | Each chart was decoded into a generic JSON tree and then decoded again. | Direct typed decoding; approximately 51% lower parsing time in repeated microbenchmarks. |
| P2 | `utils.rs` | A single long line could exceed Telegram limits; splitting changed newlines or created empty chunks. | Lossless Unicode-safe splitting, including long-line and emoji regressions. |
| P2 | API schedule handler | HTTP schedule changes did not update the live scheduler. | Uses the initialized scheduler and its shared throttled bot immediately. |
| P2 | Logging/configuration | Debug logs exposed prompts/replies; derived Debug could expose configuration secrets. | Metadata-only new LLM logs, redacted configuration Debug, no prompt/reply debug output, no query strings in HTTP tracing spans. |

## Telegram improvements

Update dispatch is retained. Expensive work has shared admission and a per-owner guard. Handler and scheduled-work deadlines prevent indefinite retries from holding application capacity. Background profile/selection work is tracked and owns its guard through completion. Summary persistence remains inside that ownership window.

All teloxide sends share one throttle worker with the crate's chat/global limits and retry-after behavior. The raw draft API uses a separate process-wide budget (at most ten draft attempts/second), approximately one update/second per active stream, and retry-after backoff. Drafts can be skipped under load. These are separate budgets, not a claim that one universal limiter covers every Telegram method. Telegram behavior was not stress-tested against the real API.

The stream path no longer accumulates a second complete LLM response solely for logging. Cancellation reaches the producer, blocked sends are bounded, and empty/interrupted responses are not presented as successful complete analyses. New history-byte limits deliberately omit oversized context turns. Actual Telegram end-to-end latency, 429 behavior against the live service, CPU utilization, and allocation counts were not measured.

## WebSocket implementation

The gateway supports `get_state`, idempotent `set_model`, `subscribe(profile.updated)`, and `unsubscribe`, with explicit version and request IDs. It exposes fixed owner capabilities, not arbitrary Telegram targets, raw updates, or delegated scopes. State reads return profile presence/model/schedule, while existing authenticated APIs serve analysis functionality.

Default controls: 256 connections globally, two per owner, 16 KiB inbound frames/messages, 32 queued notifications, one executing command per connection, one permitted subscription, 120 received requests/frames per owner per fixed minute, 60-second heartbeat timeout, five-second sends/commands, and a 30-second process drain. HTTP work and upstream LLM concurrency each have separate caps. Runtime configuration is centralized and validated.

The gateway rechecks credentials before commands and on heartbeat; the legacy fortune socket also rechecks before generation and during streaming. Origin checks are explicit. Slow consumers disconnect; publishers never wait for sockets. Events carry an ID and source, are ephemeral, and require resubscription plus a fresh state read after reconnect. Request IDs correlate results; there is no durable deduplication or exactly-once guarantee. Repeating `set_model` is safe but can repeat its notification.

Production TLS terminates at a trusted reverse proxy. The application binds loopback by default. An [nginx example](../deploy/nginx.conf.example) supplies the deployment boundary but was not deployed or validated with nginx/certificates here. Full protocol details, close codes, browser restrictions, migration behavior, and all limits are in [WebSocket integration](websocket-integration.md).

## Performance changes and measurements

| Before | Bottleneck | Modification | Result classification |
| --- | --- | --- | --- |
| Generic JSON tree followed by typed conversion | Extra parse representation and allocations | Deserialize chart directly | **Measured:** median 195.60 ms → 95.76 ms per 20,000 fixture decodes across the final ten repetitions; 51.0% lower elapsed time |
| New gateway with default 128 KiB transport buffers | Large resident buffers at high connection count | 8 KiB reads, eager writes, explicit write ceiling | **Measured:** combined process RSS at 1,000 clients 167.9 MiB → 44.5–51.4 MiB across tuned runs; not a server-only per-connection measurement |
| Delayed small writes | Possible TCP batching latency | TCP_NODELAY on accepted sockets | **Measured but noisy:** no isolated attributable speedup; retain for incremental request/response behavior |
| Full response copy retained for logging | Duplicate growing response buffer | Redacted metadata logging | **Statically inferred:** removes that response copy; allocations were not instrumented |
| Cache queue 1,024 turns, 64 MiB page cache per connection × eight | Loose memory budgets | 128 turns × 64 KiB; 8 MiB page cache × four | **Statically inferred:** lower configured ceilings; RSS impact not isolated |
| Unbounded admitted expensive work | External API and task pressure | Shared operation/LLM caps and cancellation | **Statically inferred:** bounded concurrency, not a claim of faster saturated throughput |

Final local WS runs (20 sequential state requests per client, concurrent clients; two repetitions):

| Clients | Requests/run | Requests/sec | p50 latency | p99 latency | Combined process RSS after connection establishment |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 20 | 7,688–8,468 | 0.111–0.119 ms | 0.186–0.430 ms | 15.4 MiB |
| 10 | 200 | 3,561–3,951 | 0.315–0.354 ms | 43.4–47.1 ms | 16.6–16.8 MiB |
| 100 | 2,000 | 21,868–22,440 | 2.100–2.136 ms | 39.7–41.9 ms | 20.4–20.6 MiB |
| 1,000 | 20,000 | 33,186–34,068 | 24.445–24.966 ms | 66.6–68.1 ms | 44.5–44.6 MiB |

Environment: local x86_64, 16 visible CPUs, Rust 1.98.1, four Tokio workers in the load test, release `opt-level=z`, in-memory SQLite, loopback sockets. The harness raises the connection limit to 1,200 for the 1,000-client case; deployment default remains 256. Both client and server run in one process. Throughput excludes connection establishment; latency includes request processing and scheduling. RSS is sampled before requests, includes the client, and excludes kernel socket memory. Small-sample 10-client tails are noisy and remain visible rather than omitted.

There is no original-repository gateway throughput baseline: that endpoint did not exist. The initial transport-default measurement applies to the newly implemented gateway. No production Telegram/LLM benchmark, ARM benchmark, allocation profiler, CPU profile, sustained soak, or TLS load result is claimed.

The [recorded benchmark output](benchmark-results-2026-09-11.txt) preserves individual trials. Reproduce with:

```sh
cargo test --release --test performance_tests --locked -- --ignored --nocapture --test-threads=1
```

## Verification

| Check | Result |
| --- | --- |
| `cargo check --all-targets --locked --offline` | Passed |
| `cargo fmt --all -- --check` | Passed; rustfmt emits a non-failing width-configuration warning |
| `cargo clippy --all-targets --all-features --locked --offline -- -D warnings` | Passed |
| `cargo test --all-targets --locked --offline` | 17 passed; two opt-in performance tests ignored by this command |
| Release performance command above | Both opt-in tests passed in multiple runs, including 1/10/100/1,000 clients |
| `cargo build --release --locked --offline` | Passed |
| Mermaid sequence syntax | All six parsed successfully with cached Mermaid in headless Chrome |
| `git diff --check` | Passed |
| Dependency graph | Reviewed with `cargo tree`; lockfile changes inspected |
| Vulnerability database audit | Not run: `cargo-audit` is not installed |
| Proxy/TLS validation | Not run: nginx/certificates/deployment environment unavailable |
| Fuzzing/sanitizers | Not run; deterministic malformed/deep input regressions were added |

The original baseline had seven passing unit tests, then stopped because sandbox policy prevented local sockets. Subsequent mock/WS tests ran with loopback execution permission. Existing HTTP/chart fixtures were incomplete and used endpoints unrelated to production requests; corrected them and injected trusted test endpoints. All integration tests now avoid real Telegram, almanac, chart, and LLM services.

## Regression re-audit

Re-read admission, processing guards, event routing, socket loops, LLM producer/consumer completion, primary/cache writes, and shutdown after the changes. Confirmed no DashMap guard spans the previously identified network awaits; processing ownership survives detached work and cancellation; no websocket client chooses an owner; subscriber queues are bounded; failed streams do not emit success; key rotation is checked on established sockets; final socket cleanup releases permits. The first-cycle arithmetic regression and malformed-SSE test cover additional correctness faults found during this pass.

SQLite remains the serialized write bottleneck at larger scale. The rate-table mutex is short-held and bounded, but could become a contention point beyond these tests. Slow subscriber delivery is isolated from Telegram publication. Telegram throttling waits inside admitted, time-bounded work rather than allowing unlimited expensive jobs. No unsafe code or speculative general-purpose event bus was introduced.

## Material files changed

- `src/api/{admission,auth,gateway,handlers,charts,mod}.rs`: admission, versioned protocol, compatibility hardening, chart capabilities, listener behavior.
- `src/models/{runtime,events,processing_guard,state,common,mod}.rs`: resource budgets, owner events, guard lifecycle, bounded history, explicit stream completion.
- `src/config/{mod,runtime}.rs`, `.env.example`: validated defaults, trusted test endpoints, redacted Debug.
- `src/bot/{mod,commands,callbacks,messages,helpers,command_actions}.rs`, `src/main.rs`, `src/scheduler.rs`: shared throttle, private credentials, deadlines, tracked work, shutdown and live schedules.
- `src/services/{llm,http,integration,almanac,bazi_service,mod}.rs`, `src/services/paipan/{client,formatter,bazi_utils}.rs`, `src/utils.rs`: bounded upstream handling, metadata logging, stream correctness, application actions, HTML escaping, parsing and splitting fixes.
- `src/repos/{mod,chat_history_cache}.rs`, `migrations/20260910000001_chart_tokens.sql`: migration integrity, SQLite IDs, credentials, capabilities, bounded cache/barriers.
- `tests/*`, new fixture/load/security tests, `Cargo.toml`, `Cargo.lock`, `rustfmt.toml`, `.github/workflows/check.yml`: reproducible verification and focused dependency features.
- Protocol, project sequence, deployment, audit documentation and nginx example.

## Remaining risks and next actions

| Severity | Reason | Impact | Recommended action |
| --- | --- | --- | --- |
| P1 deployment prerequisite | Application TLS and half-open TCP/IP admission are delegated to the proxy. | Direct public plaintext exposure or missing proxy timeouts defeats the intended boundary; slow SSE clients need downstream write deadlines. | Configure/test TLS, firewalling, connection/IP limits, and header/body/write timeouts before production. |
| P2 | Fixed owner keys, in-process limits/events, SQLite, and global state. | No delegated app scopes, distributed rate coordination, cross-instance event delivery, or seamless horizontal scaling. | Introduce delegated credentials/shared infrastructure only when actual multi-instance or delegation requirements exist. |
| P2 | Provider SDK parses a full non-streaming LLM response before the post-parse size check; chart/almanac responses are bounded before parsing. | A malicious/misconfigured trusted LLM provider can still send an oversized full body. | Enforce provider/proxy body limits or add a bounded custom SDK transport if that trust boundary changes. |
| P2 | Full chart/storage and profile updates are not a distributed transaction; some inherited repository writes still log errors rather than returning them. | Storage failures can leave a profile with an unavailable chart or missing derived data. | Add explicit reconciliation/transactional publication if stronger end-to-end persistence guarantees are required. |
| P2 | No durable event replay; notifications only cover chart capability/model updates. | Disconnects miss events; other profile fields/schedule updates require a fresh read. | Follow documented reconnect/snapshot behavior; add topics only for concrete client needs. |
| P2 | Structured logs are available, but no metrics exporter, long soak, chaos run, or live Telegram/TLS verification was performed. | Operational confidence and capacity estimates are limited. | Add low-cardinality metrics and run staging/ARM/real-proxy soak tests before sizing production. |
| P2 | Historical DB logs can retain sensitive content; log-table retention is not automatic. | Existing privacy/storage burden remains. | Apply an explicit retention/redaction policy to historical data and monitor DB growth. |
| P2 | Shutdown is deadline bounded; process crash/forced exit can lose work and disposable cache turns. | No exactly-once or fully durable job-delivery guarantee. | Use durable jobs only if required; monitor deadline-expiry logs. |
| P3 | Duplicate networking/TLS dependencies remain; MSRV and RustSec audit are not certified. | Build size/time and dependency-risk uncertainty. | Audit advisories and test the supported ARM/MSRV matrix before changing dependencies. |

## Final scores

These are engineering judgments for the current verified scope, not certification scores.

| Area | Score | Basis |
| --- | ---: | --- |
| Rust implementation quality | 8/10 | Strict linting, safer ownership and regressions; inherited global state/persistence gaps remain |
| Telegram efficiency | 7/10 | Shared sends and bounded drafts/work; no live upstream benchmark |
| WebSocket architecture | 8/10 | Typed owner boundary, lifecycle and backpressure; intentionally narrow protocol |
| Third-party integration readiness | 7/10 | Documented tested protocol; delegated credentials/TLS deployment remain |
| Security | 7/10 | Concrete credential, XSS, admission, and chart fixes; external deployment/advisory verification needed |
| CPU efficiency | 7/10 | Measured parser improvement; no CPU profile |
| Memory efficiency | 8/10 | Measured buffer reduction and explicit budgets; no allocation instrumentation |
| Async/concurrency efficiency | 8/10 | Guard ownership, cancellation, bounded work, shared pools |
| Scalability | 6/10 | Local 1,000-client exercise; single-node storage/events/limits |
| Observability | 6/10 | Safer structured logs; no exporter or staging dashboards |
| Maintainability | 8/10 | Existing boundaries preserved, shared controls, protocol/tests/docs aligned |
| Production readiness | 7/10 | Useful verified single-node foundation; proxy, staging, retention and soak work remain |
