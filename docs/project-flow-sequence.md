# BaziFlowAgent project flow

This document reflects the effective working tree reviewed on 2026-09-11. It covers process startup, profile creation, interactive readings/chat, and scheduled daily delivery.

## 1. Process startup and request surfaces

```mermaid
sequenceDiagram
    title Process startup and request surfaces
    participant Process
    participant PrimarySQLite
    participant ChatCache
    participant SharedState
    participant Scheduler
    participant TelegramDispatcher
    participant AxumServer

    Process->>PrimarySQLite: Load configuration and apply migrations
    Process->>ChatCache: Open cache database and start writer
    Process->>SharedState: Build HTTP, database, R2, and LLM state
    Process->>Scheduler: Load active schedules and start cron jobs
    Scheduler->>PrimarySQLite: Read scheduled users
    PrimarySQLite-->>Scheduler: User schedules and profiles
    Process->>TelegramDispatcher: Register commands and update handlers
    Process->>AxumServer: Bind authenticated API and chart capability routes
    TelegramDispatcher-->>Process: Telegram updates handled
    AxumServer-->>Process: HTTP, SSE, and WebSocket requests handled
```

The process also starts log/context cleanup jobs and runs both request surfaces until graceful shutdown. The scheduler owns a `Bot` instance for centrally throttled scheduled Telegram delivery; it does not route scheduled messages through the update dispatcher.

## 2. Profile creation and initial Bazi analysis

```mermaid
sequenceDiagram
    title Profile creation and initial Bazi analysis
    participant InteractiveClient
    participant TransportHandler
    participant ContextStore
    participant BaziService
    participant PaipanAPIs
    participant PrimarySQLite
    participant ChartStorage
    participant LLMGateway

    InteractiveClient->>TransportHandler: Submit gender and birth date with optional time and location
    TransportHandler->>ContextStore: Clear prior conversation
    TransportHandler->>BaziService: Prepare Bazi data
    alt Birth time provided
        BaziService->>BaziService: Use the validated birth time
    else Birth time omitted
        BaziService->>BaziService: Use 12:00 reference and mark time unknown
    end
    opt Birth location provided
        BaziService->>BaziService: Apply true solar time longitude correction
    end
    BaziService->>PaipanAPIs: Fetch base chart
    PaipanAPIs-->>BaziService: Base pillars and luck cycles
    BaziService->>PaipanAPIs: Fetch supplementary data concurrently
    PaipanAPIs-->>BaziService: Relations, Yongshi, and Shensha
    BaziService->>PrimarySQLite: Upsert structured profile
    BaziService->>ChartStorage: Rotate chart capability and publish escaped HTML
    BaziService->>LLMGateway: Request destiny analysis and summary
    LLMGateway-->>BaziService: Analysis stream and compact summary
    BaziService->>PrimarySQLite: Save completed analysis, summary, and redacted LLM metadata
    BaziService-->>TransportHandler: Chart URL and analysis result
    TransportHandler-->>InteractiveClient: Telegram, JSON, or SSE response
```

`InteractiveClient` represents either a Telegram user or an authenticated API client. `TransportHandler` represents the relevant teloxide handler or Axum endpoint. Birth time and birth location can both be omitted. An omitted time uses 12:00 only as a date-safe reference for the required upstream chart calculation; the stored profile marks the time as unknown, and consumers are warned that the reference hour pillar is uncertain. An omitted location receives no longitude correction. `ChartStorage` is Cloudflare R2 when configured and the local `public/` directory otherwise.

## 3. Interactive fortune, date selection, and follow-up chat

```mermaid
sequenceDiagram
    title Interactive fortune, date selection, and follow-up chat
    participant InteractiveClient
    participant TransportHandler
    participant PrimarySQLite
    participant MemoryContext
    participant ChatCache
    participant CoreServices
    participant MingDecodeAPI
    participant LLMGateway

    InteractiveClient->>TransportHandler: Send date, range, or chat request
    TransportHandler->>PrimarySQLite: Authenticate and load profile settings
    TransportHandler->>MemoryContext: Rate limit and update session state
    MemoryContext->>ChatCache: Restore recent turns when needed
    TransportHandler->>CoreServices: Build personalized request
    CoreServices->>MingDecodeAPI: Fetch one date or date range when required
    MingDecodeAPI-->>CoreServices: Validated and formatted almanac data
    CoreServices->>LLMGateway: Send prompt with chart, summary, almanac, and history
    LLMGateway-->>CoreServices: Full response or streamed chunks
    CoreServices->>PrimarySQLite: Append redacted LLM request metadata
    CoreServices->>MemoryContext: Store assistant turn
    MemoryContext->>ChatCache: Queue write-behind persistence
    CoreServices-->>TransportHandler: Full or streaming result
    TransportHandler-->>InteractiveClient: Telegram drafts, JSON, SSE, or WebSocket frames
```

Almanac retrieval applies to daily-fortune and date-selection requests. Follow-up chat skips that call and uses the stored structured chart, compact Bazi summary, and recent conversation turns.

## 4. Scheduled daily fortune delivery

```mermaid
sequenceDiagram
    title Scheduled daily fortune delivery
    participant Scheduler
    participant PrimarySQLite
    participant AlmanacService
    participant MingDecodeAPI
    participant LLMGateway
    participant TelegramBot
    participant TelegramUser

    Scheduler->>PrimarySQLite: Load active schedules at startup
    Scheduler->>PrimarySQLite: Load user profile at trigger time
    Scheduler->>AlmanacService: Request tomorrow fortune
    AlmanacService->>MingDecodeAPI: Fetch tomorrow almanac
    MingDecodeAPI-->>AlmanacService: Validated almanac data
    AlmanacService->>LLMGateway: Generate personalized daily reading
    LLMGateway-->>AlmanacService: Full reading
    AlmanacService->>PrimarySQLite: Save LLM request log
    AlmanacService-->>Scheduler: Almanac and reading
    Scheduler->>TelegramBot: Send Unicode-safe chunks through shared throttle
    TelegramBot-->>TelegramUser: Tomorrow almanac and fortune
```

The same scheduler also expires stale in-memory contexts and removes old log files on configured cron expressions.

## 5. Persistent third-party gateway

```mermaid
sequenceDiagram
    title Owner-scoped persistent integration
    participant clientApp
    participant admission
    participant gateway
    participant appService
    participant primarySQLite
    participant eventHub

    clientApp->>admission: Upgrade with Authorization header
    admission->>primarySQLite: Look up credential hash
    admission->>gateway: Admit owner within global and per-owner limits
    clientApp->>gateway: Versioned request and correlation ID
    gateway->>primarySQLite: Revalidate credential
    gateway->>appService: Get owner state or set model
    appService->>primarySQLite: Owner-scoped query or idempotent assignment
    appService->>eventHub: Publish profile notification
    gateway-->>clientApp: Correlated response
    eventHub-->>gateway: Bounded queue for subscribed owner
    gateway-->>clientApp: Ephemeral profile.updated event
    gateway->>clientApp: Native Ping and deadline
    clientApp-->>gateway: Pong
```

The gateway has no Telegram command passthrough. Its fixed capabilities are owner state reads, model assignments, and profile notifications. Invalid actions cannot select another owner/chat. Queue saturation cancels the slow connection; publisher work never awaits a subscriber. `/api/v1/date-fortune` remains a separate, bounded, single-reading compatibility protocol. See [protocol and limits](websocket-integration.md).

## 6. Shutdown and cleanup

```mermaid
sequenceDiagram
    title Bounded process shutdown
    participant process
    participant telegramDispatcher
    participant scheduler
    participant axumServer
    participant trackedTasks
    participant chatCache
    participant databases

    process->>process: Receive SIGINT or SIGTERM or service exit
    process->>axumServer: Stop acceptance and cancel sockets and streams
    process->>telegramDispatcher: Stop updates and cancel handlers
    process->>scheduler: Stop future scheduled jobs
    process->>trackedTasks: Close tracker and wait for active work
    process->>chatCache: Flush queued writes using barrier
    process->>databases: Close pools
    process->>process: Drop logging guards and stop throttle worker
```

Shutdown has a configured total drain deadline. Deadline expiry is logged and remaining service tasks are aborted as the runtime exits. In-memory contexts that are processing cannot be expired by cleanup. Telegram detached profile/selection work retains its admission guard until completion. HTTP streaming holds admission until its body finishes or is dropped. LLM producers stop on receiver cancellation, shutdown, output limit, or blocked-send deadline. Completed-stream acknowledgement is required before saving an analysis or sending success. HTTP schedule updates apply immediately through the shared scheduler.

Local chart retrieval resolves a 256-bit capability token to its owner in SQLite and serves the corresponding escaped HTML with no-store and no-referrer headers. It does not expose the filesystem directory. Caches are disposable; oversized history turns and full cache queues are rejected observably.

## Maintenance rule

When a code change affects any entry point, handler, service, external API, datastore, chart storage path, streaming protocol, scheduler job, or response path, update the affected sequence above in the same change. Add a new focused sequence when an independent flow cannot be represented accurately in an existing one.
