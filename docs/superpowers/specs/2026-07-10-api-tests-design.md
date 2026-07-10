# BaziFlowAgent API Tests Design

## Overview
This document outlines the design for the integration tests of the BaziFlowAgent API (`src/api`). The goal is to comprehensively test all endpoints in `src/api` while keeping external dependencies isolated using mockito and in-memory SQLite.

## Architecture

We will create a new integration test file at `tests/api_tests.rs`. 
For the setup, we'll create a reusable helper function `setup_test_app()` that:
- Initializes an in-memory SQLite database and runs migrations.
- Seeds a dummy user with an API key (e.g., `user_id = 1`, `api_key = "test_token"`) to satisfy `AuthUser`.
- Starts a `mockito::Server` to mock external API calls (MingDecode API and OpenAI-compatible LLM endpoint).
- Builds the `AppState` and returns the `Router` configured with our API routes.

## Component Breakdown

For each endpoint, we will use `axum::test` (via `tower::ServiceExt::oneshot`) to dispatch HTTP requests directly to the router without binding to a port.

1. **`POST /api/v1/profile`**: Test successful profile creation (mocks LLM and returns URL & analysis), invalid inputs (e.g., wrong date format), and SSE streaming mode (`?stream=true`).
2. **`GET /api/v1/profile`**: Test fetching the profile after creation, and the 404 error when no profile exists.
3. **`POST /api/v1/date-fortune`**: Test the fortune generation with mock MingDecode calendar data and mock LLM analysis. Test both standard JSON and SSE streaming.
4. **`POST /api/v1/pick-date`**: Test date selection logic, validating input dates, and ensuring the LLM streaming/JSON endpoints behave correctly.
5. **`PUT /api/v1/model`**: Test updating the LLM model and verify the DB updates.
6. **`PUT /api/v1/schedule`**: Test updating the user schedule with valid HH:MM format and invalid formats.
7. **`POST /api/v1/chat`**: Test conversational LLM interactions, ensuring message history is tracked in the `AppState` context, and verify SSE streaming.

## Testing Rules (TDD context)
Since the `src/api/handlers.rs` code is already fully implemented, strict TDD is being relaxed as per user agreement. However, we will still apply TDD principles by:
- Writing the test for the expected behavior.
- Watching the test fail (e.g., initially by asserting the wrong status or by missing mocks).
- Fixing the test setup to ensure it passes with the existing implementation.
