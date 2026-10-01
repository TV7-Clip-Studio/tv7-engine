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
- Browser calls from local Studio are allowed only for `http://127.0.0.1:8080` and `http://localhost:8080` (CORS); Engine still binds to loopback only. CORS does not authenticate callers on the local machine.
- Local transcript generation requires explicit `TV7_ENGINE_WHISPER_BIN` and `TV7_ENGINE_WHISPER_MODEL` setup; Engine reports `TRANSCRIPTION_SETUP_REQUIRED` instead of downloading models silently.
- Studio migration details live in `docs/core/contracts/tv7-engine/studio-sidecar-migration.md`.

## Run Locally
```bash
make dev
```

## Selected YouTube Channel Flow

- `POST /v1/channels/discover` with `{"url":"https://www.youtube.com/@creator/videos","cursor":0}` lists up to 25 videos. `next_cursor` can be sent for another page; discovery stops at 500 entries and never downloads media.
- A channel handle URL such as `https://www.youtube.com/@creator` is normalized to its `/videos` tab. A first page with no videos is reported as an error so the UI can show yt-dlp's diagnostic instead of treating it as a successful empty channel.
- `POST /v1/ingest/selected` with `{"videos":[{"id":"abcdefghijk","url":"https://www.youtube.com/watch?v=abcdefghijk"}]}` starts one job per selected video and returns `jobs` with `video_id` and `job_id`. The optional `output_dir` must be an existing directory inside Engine's asset directory. Selection is capped at 25 videos per request.
- `POST /v1/ingest/url` with `{"url":"https://www.youtube.com/watch?v=abcdefghijk"}` starts a single download job. Its optional `output_dir` follows the same Engine asset directory restriction.
- Poll `GET /v1/jobs/:id` and cancel with `POST /v1/jobs/:id/cancel`. At most three URL downloads run concurrently; the rest stay queued. Cancellation terminates an active `yt-dlp` download and removes files produced with that job's output prefix. Jobs remain in memory and are not resumed after Engine restart.
- `POST /v1/ingest/file` accepts one multipart `file` field up to 1 GiB. Engine streams it into its asset directory and returns `media_id`, `media_path`, and `size_bytes`; browser and desktop clients use the same byte handoff. Existing media APIs still require the Engine-owned `media_path`. The `media_id` prepares a future path-free media contract.

Downloads are user-initiated for media the user is authorized to download. The Engine binds to loopback; exact browser origins are allowed by CORS, but CORS is not local-process authentication.

## Local Transcript Candidates And Captioned Render

- `POST /v1/transcripts/candidates` accepts an Engine-owned VTT `transcript_path`, optional `max_candidates` (1–12, default 8), and optional `max_duration_seconds` (15–90, default 45). It returns chronological cue ranges sampled across the transcript with excerpt text and cue counts. These are deterministic review candidates, not viral predictions or quality scores.
- `POST /v1/media/render-clip` accepts optional `subtitle_path` pointing to an Engine-owned VTT. Engine shifts overlapping cues to the clip timeline, burns them into the video with FFmpeg, and cleans up its temporary subtitle file. A clip with no overlapping cues returns an error.
- Candidate generation reads at most 5 MiB and 20,000 cues. The Engine HTTP boundary canonicalizes VTT paths and accepts them only under its asset directory.
- `POST /v1/transcripts/import` accepts one multipart `.vtt` `file` field (up to 5 MiB). Engine validates timed cues before atomically storing the transcript in its asset directory. The response provides `transcript_id`, `transcript_path`, `cue_count`, and `duration_seconds`; browser and desktop clients pass `transcript_path` to candidate generation and captioned rendering.

## Approved Clip Export

- A successful `POST /v1/media/render-clip` response includes `result.media_id` when the output is an MP4 inside Engine's asset directory and is at most 1 GiB. Other render results keep `result.output_path` but have no export ID.
- After creator approval, Studio can fetch `GET /v1/media/assets/:id` and upload the returned `video/mp4` bytes to Backend. The route accepts a single HTTP byte range and returns 206 for partial content or 416 for an invalid range. It requires the same session bearer token as other Engine routes when `TV7_ENGINE_API_TOKEN` is set.
- Export IDs are opaque, held in memory for up to one hour (at most 100), and disappear on Engine restart. This is a local proof boundary; durable approved exports require a persistent asset catalog.

Environment variables:
- `TV7_ENGINE_HOST` (default: `127.0.0.1`, must be loopback)
- `TV7_ENGINE_PORT` (default: `7070`)
- `TV7_ENGINE_LOG_LEVEL` (default: `debug` via `make dev`)
- `TV7_ENGINE_WORK_DIR` (default: current working directory, must be absolute existing directory)
- `TV7_ENGINE_ASSET_DIR` (default: `$TV7_ENGINE_WORK_DIR/.tv7-engine-assets`, must be absolute when set)
- `TV7_ENGINE_FFMPEG_BIN` (default: `ffmpeg`)
- `TV7_ENGINE_FFPROBE_BIN` (default: `ffprobe`)
- `TV7_ENGINE_YT_DLP_BIN` (default: `yt-dlp`)
- `TV7_ENGINE_YT_DLP_JS_RUNTIME` (optional: `node`, `deno`, an absolute runtime path as `node:/path/to/node` or `deno:/path/to/deno`, or `none`; defaults to a supported installed Deno >=2.3 or Node >=22). Engine passes the selected runtime as `--js-runtimes` to yt-dlp and reports missing runtime in diagnostics and setup status. Engine never installs one silently.
- `TV7_ENGINE_WHISPER_BIN` (optional user-installed Whisper-compatible runtime path/name)
- `TV7_ENGINE_WHISPER_MODEL` (optional absolute path to a user-provided Whisper model)
- `TV7_ENGINE_OLLAMA_BIN` (default: `ollama`)
- `TV7_ENGINE_OLLAMA_MODEL` (optional user-installed Ollama model name)
- `TV7_ENGINE_CLOUD_AI_AVAILABLE` (default: `true`; set `false` when TV7 cloud AI is unavailable in the current environment)
- `TV7_ENGINE_BACKEND_BASE_URL` (optional Backend base URL for TV7 cloud AI execution)
- `TV7_ENGINE_API_TOKEN` (optional in development; packaged Studio generates a fresh 32–128 character visible ASCII token per Engine session). When set, every Engine route including `/health` requires `Authorization: Bearer <token>`. Missing or wrong credentials return HTTP 401 `ENGINE_AUTH_REQUIRED` before any route action. Browser CORS preflight permits the `Authorization` header. Engine never prints the token; diagnostics responses include `x-tv7-engine-auth-mode: token_required` or `development_unprotected`. Running without a token logs a development warning.

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
