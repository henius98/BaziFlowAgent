# Third-party integration protocol

Persistent endpoint: `GET /api/v1/ws`.
Use `wss://` in production through trusted TLS reverse proxy.
Rust listener defaults to `127.0.0.1:8080`.
`ws://` local development only.
Never expose plaintext listener directly to internet.
Firewall non-loopback listener to proxy only.
Proxy must enforce connection/IP limits, handshake/header timeouts, and idle timeout above `WS_IDLE_SECONDS`.
App does not trust `X-Forwarded-For`.
App rejects credentials in URLs.

## Authentication and authorization

Send `Authorization: Bearer <api_key>` in HTTP upgrade.
Generate and rotate owner keys via `/apikey` in bot private chat.
Stores only SHA-256 hashes of random 128-bit keys.
Every action runs as key owner.
No request can select user ID or Telegram chat.
Keys have fixed capabilities: `state.read`, `model.write`, `profile.events.subscribe`.
No Telegram send/edit/delete capability.
No raw-update subscription.
Use separate users for separate ownership.
No delegated per-app keys or narrower scopes.

Key revalidates before each request and heartbeat.
Key rotation disconnects sockets on next request or heartbeat, not instantly.
Handshake identity limiter persists across reconnects.
Native clients omit `Origin`.
Browser requests require exact configured `CORS_ALLOWED_ORIGIN`.
Wildcards do not authorize browser WebSocket origins.
Native browser WebSocket cannot set upgrade headers.
Use trusted server-side integration instead of keys in URLs.

## Requests

```json
{"version":1,"id":"req-123","action":{"name":"get_state"}}
{"version":1,"id":"req-124","action":{"name":"set_model","model":1}}
{"version":1,"id":"req-125","action":{"name":"subscribe","event":"profile.updated"}}
{"version":1,"id":"req-126","action":{"name":"unsubscribe"}}
```

`id` is 1–64 ASCII letters/digits or `-_.`. Unknown fields, actions, or protocol versions are rejected. Model IDs use the same provider mapping as Telegram and HTTP (`LlmModel`); selectable IDs are the enabled entries in the durable `llm_models` setting. `get_state` returns `has_profile`, `model`, and the stored schedule cron expression. It does not return private conversation content. One subscription is available: `profile.updated` for the authenticated owner. Subscriptions apply only to future notifications.

```json
{"version":1,"type":"response","id":"req-123","ok":true,"data":{"has_profile":true,"model":1,"schedule":null}}
{"version":1,"type":"error","id":"req-125","error":{"code":"subscription_not_allowed"}}
{"version":1,"type":"event","event":"profile.updated","event_id":"019...","source":"application"}
```

Parse failures use `id: null`. Error codes include `invalid_request`, `unsupported_version`, `invalid_id`, `invalid_model`, `subscription_not_allowed`, `storage_unavailable`, and `request_timeout`. Responses and events can interleave: correlate responses by ID. Events notify clients to reload state; they contain no raw Telegram data or message bodies. They are published when chart capability generation or a shared model update succeeds. Other profile fields/schedule changes do not currently emit notifications.

## Delivery and reconnects

Events are ephemeral, best effort, and not replayed. Reconnect, authenticate, subscribe, and fetch a fresh snapshot. Event IDs identify individual notifications, not persistent replay offsets. Request IDs correlate responses, not durable deduplication records. Retrying `set_model` repeats the same assignment safely, but may emit another notification. There is no exactly-once claim. No protocol command sends a Telegram message, so an automatic Telegram-message feedback loop is absent.

## Limits and lifecycle

| Budget                                                   |     Default | Configuration              |
| -------------------------------------------------------- | ----------: | -------------------------- |
| WebSocket connections, both endpoints combined           |         256 | `WS_MAX_CONNECTIONS`       |
| Connections per owner                                    |           2 | Fixed                      |
| HTTP requests executing/streaming                        |          64 | `API_MAX_REQUESTS`         |
| Expensive application operations / upstream LLM requests |     16 each | `APP_MAX_WORK`             |
| HTTP JSON body / WS frame and assembled message          |      16 KiB | `API_MAX_MESSAGE_BYTES`    |
| Notification queue per connection                        |          32 | `WS_QUEUE_CAPACITY`        |
| Requests/frames per owner per 60-second window           |         120 | `API_REQUESTS_PER_MINUTE`  |
| Pending application commands per socket                  |           1 | Sequential execution       |
| Subscriptions per socket                                 |           1 | Owner profile notification |
| Heartbeat timeout                                        |  60 seconds | `WS_IDLE_SECONDS`          |
| Send/command timeout                                     |   5 seconds | `WS_SEND_SECONDS`          |
| HTTP work / legacy WS lifetime                           | 180 seconds | `APP_WORK_SECONDS`         |
| Shutdown drain deadline                                  |  30 seconds | `SHUTDOWN_SECONDS`         |

Server sends Ping every 1/3 of idle deadline (min 1 second).
Client must reply Pong.
Every received frame drains identity budget.
Close codes: 1008 over budget, 1013 slow consumer, 1003 unsupported frame, 1001 shutdown.
Close reasons: `heartbeat_timeout`, `credential_revoked`, `rate_limit_exceeded`, `slow_consumer`, `server_shutdown`.
Close frame delivery is best-effort.
Upgraded sockets stay tracked until cleanup.
Send calls are bounded; queues are non-blocking.
Slow subscriber never blocks Telegram tasks.

Read buffers default to 8 KiB or message limit.
Writes flush eagerly; sockets set `TCP_NODELAY`.
Write buffer ceiling fits max JSON-escaped LLM chunk; not preallocated.

In-memory history max: 128 KiB per owner.
Turns exceeding 128 KiB drop from follow-up context.
`APP_MAX_CONTEXTS` default: 256.
Owner admission races can exceed context ceiling by up to concurrent work budget.
Cache write queue holds 128 items (max 64 KiB each).
Four SQLite connections each have 8 MiB page cache.
History clear waits for flush barrier.
Bounds exclude total process heap.

Identity limiter stores max 4,096 identities; fails closed at capacity.
Fixed-window limiter allows burst across boundary.
Proxy must manage pre-upgrade socket/IP limits; app semaphores do not bound half-open TCP sockets.

## Compatibility and migration

`/api/v1/date-fortune` keeps `generate`/`stop` protocol and single-reading behavior.
Optional `reasoning_effort` in `generate`: `none`, `minimal`, `low`, `medium`, `high`, `xhigh`.
Missing field defaults to `none` (omits parameter in upstream LLM request).
Shares connection caps, admission, frame limits, timeouts, origin checks, date validation, shutdown signal.
Rechecks credentials before generation and while streaming.
Has bounded startup wait and total lifetime.
Does not implement heartbeat or subscription protocol.
Reconnect to get new reading.
Existing HTTP/SSE endpoints remain available.
Stream failure returns error; partial results never save as complete analyses.
