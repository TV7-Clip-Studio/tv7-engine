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
  - `POST /v1/media/probe`
  - `POST /v1/media/thumbnail`
  - `POST /v1/media/render-clip`
- Engine computes locally only; no business logic or cloud calls belong here.
- Loopback-only surface; inputs must use absolute paths.
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
- `TV7_ENGINE_FFMPEG_BIN` (default: `ffmpeg`)
- `TV7_ENGINE_FFPROBE_BIN` (default: `ffprobe`)
- `TV7_ENGINE_YT_DLP_BIN` (default: `yt-dlp`)

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
