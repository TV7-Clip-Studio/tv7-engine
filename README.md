# tv7-engine (Rust)

Local-first engine that powers media and AI workloads for the TV7 app. The codebase follows a **Core + Service** model: `engine-core` hosts all heavy logic (video, audio transcription, LLM), while `engine-service` is a thin Axum HTTP sidecar on `127.0.0.1`. A future `engine-ffi` crate will expose the same core APIs to Flutter mobile via C ABI.

## Docs
- `docs/architecture.md` – Core + Service layout, runtime model, and integration expectations.
- `docs/api_spec.md` – HTTP contract for video, audio, and LLM endpoints (local-only).
- `docs/roadmap.md` – phased plan aligned with the new workspace structure.
- `docs/tv7_high_architecture.md` – broader product/system context for TV7 Core/Pro.

## Current Status
- Planning-only; workspace scaffolding is the next milestone.
- Engine computes locally only; no business logic or cloud calls belong here.
- Loopback-only surface; inputs must use absolute paths.

## Next Steps (dev)
- Initialize the Cargo workspace with `engine-core`, `engine-service`, and stub `engine-ffi`.
- Wire CLI/config defaults (port, log level, work dir) and logging/tracing.
- Add mock handlers that satisfy the API spec while delegating through core traits.
