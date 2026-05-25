# tv7-engine (Rust)

Local-first engine that powers media and AI workloads for the TV7 app. The codebase follows a **Core + Service** model: `engine-core` hosts all heavy logic (video, audio transcription, LLM), while `engine-service` is a thin Axum HTTP sidecar on `127.0.0.1`. A future `engine-ffi` crate will expose the same core APIs to Flutter mobile via C ABI.

## Docs
- `docs/core/architecture/engine_architecture.md` – core responsibilities and non-goals.
- `docs/core/roadmaps/tv7-axum-engine-roadmap.md` – phased engine roadmap.
- `docs/core/contracts/openapi/tv7-axum-engine/openapi.json` – canonical local engine HTTP contract artifact.

## Current Status
- Initial local HTTP surface is implemented in `engine-service`:
  - `GET /health`
  - `GET /v1/diagnostics`
  - `POST /v1/media/probe`
  - `POST /v1/media/thumbnail`
  - `POST /v1/media/render-clip`
- Engine computes locally only; no business logic or cloud calls belong here.
- Loopback-only surface; inputs must use absolute paths.
- Studio migration details live in `docs/core/contracts/tv7-axum-engine/studio-sidecar-migration.md`.

## Run Locally
```bash
make dev
```

Environment variables:
- `TV7_ENGINE_HOST` (default: `127.0.0.1`, must be loopback)
- `TV7_ENGINE_PORT` (default: `7070`)
- `TV7_ENGINE_LOG_LEVEL` (default: `debug` via `make dev`)
- `TV7_ENGINE_WORK_DIR` (default: current working directory, must be absolute existing directory)
- `TV7_ENGINE_FFMPEG_BIN` (default: `ffmpeg`)
- `TV7_ENGINE_FFPROBE_BIN` (default: `ffprobe`)
- `TV7_ENGINE_YT_DLP_BIN` (default: `yt-dlp`)

Useful commands:
- `make check` - compile workspace checks.
- `make test` - run unit tests.
- `make lint` - apply format + clippy fixes.
