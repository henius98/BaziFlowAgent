# Unified Multi-Transport Admission Control and Fail-Fast Load Shedding

Concurrency semaphores, per-user rate limiters, and processing guards were previously fragmented between Axum middleware and ad-hoc Telegram bot handlers. We decided to centralize admission control inside `AdmissionController` within `Runtime`, enforcing unified concurrency, 60-second sliding rate windows, single-flight user locks, and a strict fail-fast load-shedding policy across HTTP, WebSocket, and Telegram transports.

## Status

accepted

## Considered Options

- **Option 1: Unbounded in-memory queueing with FIFO/fair scheduling**. Rejected because buffering long-running LLM requests during upstream provider latency spikes compounds queue delays, causes client timeouts, and risks memory exhaustion under high concurrency.
- **Option 2: Transport-specific fragmented admission**. Rejected because Web API users and Telegram bot users share the same underlying LLM concurrency pool and database, leading to unfair resource starvation.
- **Option 3: Centralized `AdmissionController` with transport-adapted fail-fast shedding** [Chosen]. Encapsulates permit checking and RAII `AdmissionPermit` lifecycle in `Runtime`. Allows Axum to return HTTP 429 with `Retry-After` headers and Telegram to return user-friendly Chinese cooldown messages.

## Consequences

- `AdmissionController` runs at the entry point of both transports before executing expensive database or LLM calls.
- `AdmissionPermit` holds `OwnedSemaphorePermit` and `UserProcessingGuard`. Permits are held across long-running SSE streams or Telegram message editing sessions via `AdmittedResponseStream`, automatically releasing permits upon stream EOF or disconnection.
- When capacity limits or rate quotas are exhausted, requests fail fast with typed `AdmissionRejection` variants instead of waiting in unbounded memory queues.
