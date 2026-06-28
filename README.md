# tv7-engine (Rust)

Local-first engine that powers media and AI workloads for the TV7 app. The codebase follows a **Core + Service** model: `engine-core` hosts all heavy logic (video, audio transcription, LLM), while `engine-service` is a thin Axum HTTP sidecar on `127.0.0.1`. A future `engine-ffi` crate will expose the same core APIs to Flutter mobile via C ABI.

## Docs
- `docs/core/architecture/engine_architecture.md` – core responsibilities and non-goals.
- `docs/core/roadmaps/tv7-engine-roadmap.md` – phased engine roadmap.
- `docs/core/contracts/openapi/tv7-engine/openapi.json` – canonical local engine HTTP contract artifact.

## Current Status
- Initial local HTTP surface is implemented in `engine-service`:
  - `GET /health`
  - `GET /v1/diagnostics`
  - `GET /v1/setup/manifest`
  - `GET /v1/setup/status`
  - `POST /v1/ai/cloud-token`
  - `DELETE /v1/ai/cloud-token`
  - `POST /v1/ai/cloud-debug`
  - `POST /v1/ai/transcript-intelligence`
  - `POST /v1/media/probe`
  - `POST /v1/media/thumbnail`
  - `POST /v1/media/render-clip`
  - `POST /v1/ingest/url`
  - `POST /v1/transcripts/download`
  - `POST /v1/transcripts/generate`
  - `GET /v1/jobs/:id`
  - `GET /v1/jobs/:id/logs`
  - `POST /v1/jobs/:id/cancel`
- Engine owns local compute and AI dispatch. TV7 cloud AI calls go through Backend using a short-lived delegated Engine token; business logic and provider credentials stay out of Engine.
- Loopback-only surface; inputs must use absolute paths.
- Local transcript generation requires explicit `TV7_ENGINE_WHISPER_BIN` and `TV7_ENGINE_WHISPER_MODEL` setup; Engine reports `TRANSCRIPTION_SETUP_REQUIRED` instead of downloading models silently.
- Studio migration details live in `docs/core/contracts/tv7-engine/studio-sidecar-migration.md`.

## Run Locally
```bash
make dev
```

Environment variables:
- `TV7_ENGINE_HOST` (default: `127.0.0.1`, must be loopback)
- `TV7_ENGINE_PORT` (default: `7070`)
- `TV7_ENGINE_LOG_LEVEL` (default: `debug` via `make dev`)
- `TV7_ENGINE_WORK_DIR` (default: current working directory, must be absolute existing directory)
- `TV7_ENGINE_ASSET_DIR` (default: `$TV7_ENGINE_WORK_DIR/.tv7-engine-assets`, must be absolute when set)
- `TV7_ENGINE_FFMPEG_BIN` (default: `ffmpeg`)
- `TV7_ENGINE_FFPROBE_BIN` (default: `ffprobe`)
- `TV7_ENGINE_YT_DLP_BIN` (default: `yt-dlp`)
- `TV7_ENGINE_WHISPER_BIN` (optional user-installed Whisper-compatible runtime path/name)
- `TV7_ENGINE_WHISPER_MODEL` (optional absolute path to a user-provided Whisper model)
- `TV7_ENGINE_OLLAMA_BIN` (default: `ollama`)
- `TV7_ENGINE_OLLAMA_MODEL` (optional user-installed Ollama model name)
- `TV7_ENGINE_CLOUD_AI_AVAILABLE` (default: `true`; set `false` when TV7 cloud AI is unavailable in the current environment)
- `TV7_ENGINE_BACKEND_BASE_URL` (optional Backend base URL for TV7 cloud AI execution)

Useful commands:
- `make check` - compile workspace checks.
- `make test` - run unit tests.
- `make clippy` - run strict clippy checks.
- `make quality` - run the full quality gate enforced before commits.
- `make lint` - apply format + clippy fixes.

## Commit Checks
Install the local commit gate once:
```bash
pipx install pre-commit
cargo install cargo-audit --version 0.22.1 --locked
cargo install cargo-deny --version 0.18.3 --locked
pre-commit install
```

Every commit runs formatting, compile checks, tests, clippy, dependency advisories via `cargo audit`, dependency license/source policy via `cargo deny`, and basic file hygiene. Run `make quality` or `pre-commit run --all-files` to reproduce failures before committing.
