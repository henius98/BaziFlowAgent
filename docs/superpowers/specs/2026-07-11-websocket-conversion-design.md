# WebSocket Conversion for LLM Streaming

## Overview
This document outlines the design for migrating the `/api/v1/date-fortune` endpoint (and eventually other streaming endpoints like `/api/v1/pick-date`) from HTTP Server-Sent Events (SSE) to WebSockets.

## Motivation
The primary driver for this architectural change is the need for bi-directional communication over a single connection. Specifically, the client needs the ability to send a "stop" signal to halt LLM generation mid-stream, as well as the potential for future interactive follow-up messages. While SSE is excellent for one-way streaming, it requires a separate HTTP endpoint and complex state management (like a global cancellation token map) to handle abort requests. WebSockets naturally bind the request lifecycle to the connection, simplifying cancellation and enabling rich interaction.

## Architecture & Data Flow

### 1. Connection Upgrade
The existing `axum` route for `/api/v1/date-fortune` will be updated to accept WebSocket upgrade requests via `axum::extract::ws::WebSocketUpgrade`.
- **Authentication:** The current Bearer token middleware (`AuthUser`) applies to the HTTP request before the upgrade, so it will continue to work seamlessly. No custom subprotocol authentication is necessary unless the client struggles with standard HTTP headers during the WS handshake (in which case, fallback to token-in-query-string may be required).

### 2. Message Protocol
Communication will use text-based JSON frames over the WebSocket connection.

**Client to Server (Requests):**
```json
// To start generation
{
  "action": "generate",
  "date": "YYYY-MM-DD"
}

// To stop generation early
{
  "action": "stop"
}
```

**Server to Client (Responses):**
```json
// Initial context data
{
  "type": "almanac",
  "data": { ... } // Full Almanac schema
}

// LLM text stream chunk
{
  "type": "chunk",
  "data": "Here is what the Bazi says..."
}

// Stream completion
{
  "type": "done"
}

// Error state
{
  "type": "error",
  "message": "Failed to fetch almanac"
}
```

### 3. Concurrency & Cancellation
The Axum WebSocket handler will manage the stream using `tokio::select!`.

1. **Wait on WS:** The handler continuously polls for incoming messages from the client.
2. **Wait on LLM Rx:** The handler concurrently polls the MPSC receiver channel for new LLM chunks (`models::LlmResponse::Stream(receiver)`).
3. **Cancellation Logic:** If the client sends a `{"action": "stop"}` message OR if the WebSocket connection drops unexpectedly, the handler loop breaks. This cleanly drops the `receiver`, which signals the background OpenAI streaming task to terminate, saving tokens and compute.

## Verification Plan
- Send a standard `generate` WS message and verify that the almanac data and LLM chunks stream correctly.
- Send a `generate` message followed quickly by a `stop` message. Verify that the server ceases transmission and no panics or deadlocks occur.
- Hard-disconnect the client mid-stream and ensure server resources (and the LLM call) are properly cleaned up.
