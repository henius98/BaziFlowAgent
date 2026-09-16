# BaziFlowAgent API collection

Open this `bruno/` directory with **Open Collection** in Bruno. Use Bruno 4 or newer for the saved [multiple WebSocket messages](https://www.usebruno.com/v4-release).

1. Start the application with its configured database, Telegram bot and upstream services.
2. Select the **Local** environment. Adjust `base_url` and `ws_url` to your listener; use `https://` and `wss://` for a TLS deployment. Do not include a trailing slash.
3. Generate an API key with `/apikey` in the bot's private chat. Set the **secret** `api_key` variable in Bruno to the complete `bfa_...` key, without the `Bearer ` prefix. Collection authentication supplies that prefix. Secret values stay outside the collection files.
4. Edit the sample birth details, dates and other inputs in the environment. Create a profile before requesting readings or chat, unless the owner already has one.
5. Send individual requests. Profile creation replaces profile data and clears prior chat history; model and schedule requests change saved settings. Readings invoke configured upstream services. This collection is an interactive catalog, not a batch scenario.

| Folder | Requests |
| --- | --- |
| HTTP | Get/create profile, pick date, update model, enable/disable schedule, chat |
| SSE | Create profile, pick date and chat with `stream=true` |
| WebSocket | Persistent integration gateway and date-fortune stream |
| Charts | Download local HTML through `/charts/{token}` |

The collection covers all method/path pairs registered in `src/api/mod.rs` and `src/main.rs`. Request bodies follow `src/api/models.rs`; gateway messages follow `src/api/gateway.rs`. These sources remain authoritative for the API contract. Model provider names are defined in `src/models/common.rs`.

For SSE, use the response stream view. Successful streams end with `data: [DONE]`; an `error` event indicates an incomplete reading even if the HTTP status is 200. Allow sufficient request time for generation (the default server work budget is 180 seconds).

For WebSockets, connect first, then select and send a saved message. The gateway includes get-state, set-model, subscribe and unsubscribe messages. Subscribe and unsubscribe on the same connection. The date-fortune endpoint requires **Generate** first; send **Stop** on that same connection during streaming to cancel. Reconnect for another reading. Close unused sockets: the server allows two concurrent connections per owner across both endpoints. See the [integration protocol](../docs/websocket-integration.md) for responses, heartbeats and limits.

To fetch a local chart, copy the token from the latest returned `/charts/<token>` URL into Bruno's secret `chart_token` variable. The chart request explicitly disables inherited Bearer authentication because its URL is the access capability. An R2 presigned chart URL is a separate storage URL; open that full URL directly instead.

Schedule times use the server's configured application timezone (normally Asia/Singapore, UTC+8). To represent an unknown birth time, set both birth-time environment values to `null`, or omit both fields from the request. Location is optional.
