# TV7 Engine Architecture (`tv7-engine`)

## Overview
The TV7 Engine is the local compute layer of the TV7 Studio ecosystem. It runs all heavy work on the user’s machine (video, audio transcription, local LLM) with zero cloud compute. The codebase follows a **Core + Service** architecture so desktop uses an Axum sidecar today, while mobile can reuse the same core via FFI tomorrow.

- Desktop “front door”: HTTP on `127.0.0.1` (Axum).
- Mobile/CLI “front door”: future C ABI/FFI bindings calling the core directly.
- Strictly offline for compute; no business logic or cloud calls inside the engine.

Scope:
- Does: ffmpeg orchestration, whisper.cpp integration, local LLM tasks, file management, pipeline coordination, and future caching/acceleration.
- Does not: make network calls to the TV7 cloud, handle scheduling/posting/databases, or implement platform business logic (handled by FastAPI backend).

```
Flutter Desktop ──HTTP──▶ tv7-engine-service ──Rust calls──▶ tv7-engine-core
Flutter Mobile  ──FFI───────────────┘
```

## Workspace Layout
```
tv7-engine/               # Cargo workspace
  Cargo.toml
  engine-core/            # Pure logic crate (no Axum/Tokio/JSON)
    src/lib.rs
    src/video.rs
    src/audio.rs
    src/llm.rs
    src/models/
    src/error.rs
  engine-service/         # Axum HTTP sidecar
    src/main.rs
    src/http.rs
    src/handlers/
    src/config.rs
  engine-ffi/             # (Future) C ABI for mobile
    src/lib.rs
```

### Crate Responsibilities
**engine-core**
- Pure Rust logic for video processing (ffmpeg orchestration), audio transcription (Whisper/whisper.cpp), local LLM tasks (llama.cpp/other local models), and pipeline coordination.
- No dependencies on Axum/Tokio/JSON/OS-specific APIs. Keep it platform-neutral for reuse.
- Provides traits + implementations (real + mock), error types, and helpers for file/pipeline handling.

**engine-service**
- Axum HTTP API over loopback. Translates JSON requests into core calls and maps results/errors back to HTTP.
- Tokio runtime, routing, middleware, logging/tracing, config/CLI parsing, and job orchestration for long tasks.
- No business logic or cloud calls; no heavy compute lives here.

**engine-ffi** (later)
- C ABI surface for Flutter mobile (Dart → C → Rust).
- Thin adapters over `engine-core` with the same operations as the HTTP surface. Mobile never runs an Axum server.

### Why Core + Service
- Desktop sidecar now, mobile embedding later with near-zero refactor.
- Single source of truth for heavy logic; only adapters change.
- Easier testing: `engine-core` unit/integration tests run without HTTP servers.
- Clear boundaries prevent platform concerns from leaking into core logic.

## Runtime Model (Desktop)
1. Flutter spawns `tv7-engine-service` (binary) with CLI/env-configured port and log level.
2. Service binds to `127.0.0.1` only and exposes the HTTP API defined in `docs/api_spec.md`.
3. Handlers deserialize JSON, validate absolute paths, and invoke `engine-core` methods.
4. Long tasks return `202 Accepted` with `job_id`; `/jobs/{id}` polls in-memory job state.
5. Results serialize back to JSON; heavy compute and file IO happen inside `engine-core`.

## Boundaries & Dependencies
- `engine-core` must not depend on Axum, Tokio, serde_json, or platform/GUI crates.
- `engine-service` may depend on Axum/Tokio, serde/serde_json, tracing, config/clap, and job utilities.
- Feature flags control mock vs real implementations; per-domain selection is allowed for testing.
- No network calls to TV7 cloud from any crate in this workspace.

## Configuration & Lifecycle (Desktop)
- Config sources: env (`TV7_ENGINE_*`), optional `tv7_engine.toml`, CLI flags (highest priority).
- Defaults (planned): `port=27107`, `log_level=info`, `work_dir` in OS cache, optional overrides for ffmpeg/whisper/llm model paths, `mode=mock|real`, per-domain impl overrides.
- Flutter responsibilities: spawn process, poll `/health`, provide absolute paths, display progress via jobs, handle crash/restart by respawning the service.

## Observability
- `tracing` with JSON log option; request IDs propagated from handler into core calls.
- Job lifecycle logging (`queued`, `running`, `completed`, `failed`); optional Prometheus `/metrics` behind a feature flag for dev/QA.
- Structured errors from core map to HTTP error envelopes with stable codes.

## Security & Footguns
- Loopback binding only; reject non-local hosts.
- Validate and canonicalize file paths; reject traversal/symlink escapes from the work dir or user home.
- Clamp timecodes to protect ffmpeg; reject relative paths.
- Temp/work dirs scoped to engine-controlled folder; write via temp + atomic rename to avoid partial outputs.
- Job store is in-memory; clients should treat missing jobs as expired.

## Testing Strategy
- `engine-core`: unit tests for config parsing, path validation, ffmpeg command builders, whisper/llama adapters (with mocks), and pipeline helpers.
- `engine-service`: handler tests with mocked core traits to keep HTTP contract stable.
- Integration: spawn Axum server on random port, hit the API spec flows, assert side effects on temp files and job lifecycle.
- Future FFI: minimal C ABI smoke tests to ensure parity with core APIs.
