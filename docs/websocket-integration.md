# Third-party integration protocol

The persistent endpoint is `GET /api/v1/ws`. Use `wss://` in production through a trusted TLS reverse proxy. The Rust listener defaults to `127.0.0.1:8080`; `ws://` is for local development. Do not expose its plaintext listener directly to the Internet. Firewall any explicitly configured non-loopback listener to the proxy. Configure proxy connection/IP limits, handshake/header timeouts, and an idle timeout longer than `WS_IDLE_SECONDS`. The application does not trust `X-Forwarded-For` or accept credentials from URLs.

## Authentication and authorization

Send `Authorization: Bearer <api_key>` in the HTTP upgrade. Generate and rotate owner keys through `/apikey` in the bot's private chat. Only SHA-256 hashes of random 128-bit keys are stored. Every authenticated action acts as the key's owner; no request can choose a user ID or Telegram chat. Keys currently have the fixed capabilities `state.read`, `model.write`, and `profile.events.subscribe`. There is no arbitrary Telegram send/edit/delete capability or raw-update subscription. Use separate users for integrations that require separate ownership. Delegated per-application keys and narrower scopes are not implemented.

A key is revalidated before each application request and each heartbeat. Rotation disconnects existing connections at their next request or heartbeat; it is not instantaneous. The handshake's identity limiter persists across reconnects. Native clients omit `Origin`; browser requests require an exact configured `CORS_ALLOWED_ORIGIN`. A wildcard does not authorize browser WebSocket origins. Browsers cannot normally set this header through the native WebSocket constructor; use a trusted server-side integration rather than putting keys in query strings.

## Requests

```json
{"version":1,"id":"req-123","action":{"name":"get_state"}}
{"version":1,"id":"req-124","action":{"name":"set_model","model":1}}
{"version":1,"id":"req-125","action":{"name":"subscribe","event":"profile.updated"}}
{"version":1,"id":"req-126","action":{"name":"unsubscribe"}}
```

`id` is 1–64 ASCII letters/digits or `-_.`. Unknown fields, actions, or protocol versions are rejected. The model IDs use the same source of truth as Telegram and HTTP (`LlmModel::ALL`). `get_state` returns `has_profile`, `model`, and the stored schedule cron expression. It does not return private conversation content. One subscription is available: `profile.updated` for the authenticated owner. Subscriptions apply only to future notifications.

```json
{"version":1,"type":"response","id":"req-123","ok":true,"data":{"has_profile":true,"model":1,"schedule":null}}
{"version":1,"type":"error","id":"req-125","error":{"code":"subscription_not_allowed"}}
{"version":1,"type":"event","event":"profile.updated","event_id":"019...","source":"application"}
```

Parse failures use `id: null`. Error codes include `invalid_request`, `unsupported_version`, `invalid_id`, `invalid_model`, `subscription_not_allowed`, `storage_unavailable`, and `request_timeout`. Responses and events can interleave: correlate responses by ID. Events notify clients to reload state; they contain no raw Telegram data or message bodies. They are published when chart capability generation or a shared model update succeeds. Other profile fields/schedule changes do not currently emit notifications.

## Delivery and reconnects

Events are ephemeral, best effort, and not replayed. Reconnect, authenticate, subscribe, and fetch a fresh snapshot. Event IDs identify individual notifications, not persistent replay offsets. Request IDs correlate responses, not durable deduplication records. Retrying `set_model` repeats the same assignment safely, but may emit another notification. There is no exactly-once claim. No protocol command sends a Telegram message, so an automatic Telegram-message feedback loop is absent.

## Limits and lifecycle

| Budget | Default | Configuration |
| --- | ---: | --- |
| WebSocket connections, both endpoints combined | 256 | `WS_MAX_CONNECTIONS` |
| Connections per owner | 2 | Fixed |
| HTTP requests executing/streaming | 64 | `API_MAX_REQUESTS` |
| Expensive application operations / upstream LLM requests | 16 each | `APP_MAX_WORK` |
| HTTP JSON body / WS frame and assembled message | 16 KiB | `API_MAX_MESSAGE_BYTES` |
| Notification queue per connection | 32 | `WS_QUEUE_CAPACITY` |
| Requests/frames per owner per 60-second window | 120 | `API_REQUESTS_PER_MINUTE` |
| Pending application commands per socket | 1 | Sequential execution |
| Subscriptions per socket | 1 | Owner profile notification |
| Heartbeat timeout | 60 seconds | `WS_IDLE_SECONDS` |
| Send/command timeout | 5 seconds | `WS_SEND_SECONDS` |
| HTTP work / legacy WS lifetime | 180 seconds | `APP_WORK_SECONDS` |
| Shutdown drain deadline | 30 seconds | `SHUTDOWN_SECONDS` |

Native Ping frames run every one-third of the idle deadline, minimum one second. Clients must return Pong frames. Any received frame consumes the identity budget. Over-budget clients close with 1008; slow event consumers close with 1013; unsupported frames close with 1003; shutdown closes with 1001. Close reasons include `heartbeat_timeout`, `credential_revoked`, `rate_limit_exceeded`, `slow_consumer`, and `server_shutdown`. Close delivery is best effort when the peer/network has failed. Upgraded connections remain tracked until cleanup. Sending is time bounded and publishers use nonblocking bounded queues; a slow subscriber does not block Telegram work.

Transport read buffers are 8 KiB by default (or the smaller message limit), writes flush eagerly, and accepted sockets use `TCP_NODELAY`. The explicit write-buffer ceiling accommodates a maximum JSON-escaped LLM chunk; it is not preallocated.

In-memory history is limited to 128 KiB per owner; a turn exceeding that budget is omitted from follow-up context. `APP_MAX_CONTEXTS` defaults to 256. Because admission can race across distinct owners, the context count can exceed that threshold by at most the concurrent work budget. The cache write queue holds 128 entries of at most 64 KiB, and its four SQLite connections each have an 8 MiB configured page-cache budget. History clearing awaits a flush barrier. These bounds do not measure the total process heap.

The identity limiter stores at most 4,096 recent identities and fails closed at capacity. It uses fixed windows, so a boundary can permit two bursts. Socket/IP admission before an HTTP request completes belongs to the reverse proxy; application semaphore limits do not bound half-open TCP connections.

## Compatibility and migration

`/api/v1/date-fortune` retains its existing `generate`/`stop` protocol and single-reading behavior. It now shares connection limits, owner admission, frame limits, send deadlines, origin policy, date validation, and the shutdown signal. It rechecks credentials before generation and periodically during streaming. It has a bounded initialization wait and total lifetime, but does not implement the persistent gateway heartbeat/subscription protocol. Reconnect for another reading. Existing HTTP/SSE endpoints remain available. Stream failures emit an error instead of a successful completion; partial results are not persisted as completed analyses.

Old local `/bazi_<user_id>.html` URLs are intentionally unavailable. Regenerate the profile to obtain an unguessable `/charts/<token>` capability link. Anyone possessing that link can read the chart, so treat it as private. Generating a new link invalidates the old local link. Existing R2 presigned links retain their expiry semantics. Previously generated files are not deleted, but the application no longer serves the directory directly. The proxy must not independently serve `public/` or log chart capability paths.
