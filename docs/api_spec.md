# Rust Engine API Specification

**Base URL**: `http://127.0.0.1:{port}`
**Version**: `v1`

## Conventions
- Local-only HTTP API; no auth headers required while binding to `127.0.0.1`. Engine enforces loopback binding and rejects non-local hosts.
- API is served by `engine-service` (Axum) which forwards to `engine-core`; mobile bindings will use FFI instead of HTTP.
- All requests/responses use JSON with `Content-Type: application/json`.
- File paths must be absolute; the engine rejects relative paths to avoid ambiguity or traversal attacks.
- Responses are shaped as either `{ "status": "ok", ... }` or `{ "error": { ... } }`.
- Long-running work uses the async job model described below. Short operations may still return synchronously.
- CORS: not enabled (local-only). If a non-loopback host/Origin appears, the request is rejected.
- Port: default `27107`; Flutter may override via CLI/ENV and should cache the chosen port after a successful `/health`.

### Error Envelope
```json
{
  "error": {
    "code": "validation_error",
    "message": "Input file not found: /path/to/missing.mp4",
    "details": {}
  }
}
```

| code | meaning |
| --- | --- |
| `validation_error` | Request failed basic validation (missing field, bad type, relative path). |
| `io_error` | Local filesystem error (file not found, permission denied). |
| `video_processing_failed` | FFmpeg invocation failed. |
| `transcription_failed` | Whisper binding failed. |
| `llm_failed` | LLM binding failed. |
| `not_implemented` | Endpoint reachable but feature flag disabled or mock not swapped for real. |

Common status codes:
- `200`: Success
- `400`: Validation error (bad input)
- `404`: Path not found (input file)
- `500`: Internal error during processing (ffmpeg/whisper/llm crash)
- `503`: Service unavailable (backend warming up or models missing)

## General
### Health Check
- **GET** `/health`
- **Response**: `200 OK`
  ```json
  {
    "status": "ok",
    "version": "0.1.0",
    "uptime_seconds": 120
  }
  ```

---

## Video Domain

### Render Video
- **POST** `/video/render`
- **Request**:
  - `input_path` (string, required): Absolute path to source video.
  - `output_path` (string, required): Absolute path to write the rendered file. Existing file will be replaced.
  - `operations` (array, required): Sequential transforms to apply. Supported `type` values:
    - `trim`: `{ "type": "trim", "start_ms": 0, "end_ms": 5000 }`
    - `watermark`: `{ "type": "watermark", "text": "TV7", "position": "bottom_right" }`
    - `resize` (planned): `{ "type": "resize", "width": 1080, "height": 1920, "fit": "cover" }`
  - Validation: `start_ms < end_ms`, positions in `top_left | top_right | bottom_left | bottom_right | center`, `operations` must be non-empty, unknown `type` returns `400 validation_error`.
  - Output writes via temp file + atomic rename to avoid partial writes if ffmpeg fails.
  - Example:
  ```json
  {
    "input_path": "/abs/path/to/source.mp4",
    "output_path": "/abs/path/to/output.mp4",
    "operations": [
      { "type": "trim", "start_ms": 0, "end_ms": 5000 },
      { "type": "watermark", "text": "TV7", "position": "bottom_right" }
    ]
  }
  ```
- **Response**: `200 OK`
  ```json
  {
    "status": "ok",
    "output_path": "/abs/path/to/output.mp4",
    "duration_ms": 5000,
    "size_bytes": 1280000
  }
  ```
- **Notes**:
  - Default behavior: if the operation is estimated to exceed ~5–10s, the handler returns `202 Accepted` with a `job_id`; poll `/jobs/{id}` for status/progress. Small operations may return `200` directly.
  - Engine writes temp files inside its work dir before moving to `output_path`.
- **Example cURL**:
  ```bash
  curl -X POST http://127.0.0.1:27107/video/render \
    -H "Content-Type: application/json" \
    -d '{"input_path":"/tmp/in.mp4","output_path":"/tmp/out.mp4","operations":[{"type":"trim","start_ms":0,"end_ms":5000}]}'
  ```

---

## Audio Domain

### Transcribe
- **POST** `/audio/transcribe`
- **Request**:
  - `input_path` (string, required): Absolute path to media.
  - `language` (string, optional): `auto` (default) or ISO-639-1 code.
  - `model` (string, optional): `tiny`, `base`, `small`, `medium`, `large` (depends on installed models).
  - `timestamps` (bool, optional, default `true`): Include per-segment timing.
  - Example:
  ```json
  {
    "input_path": "/abs/path/to/media.mp4",
    "language": "auto",
    "model": "base"
  }
  ```
- **Response**: `200 OK`
  ```json
  {
    "status": "ok",
    "language": "en",
    "segments": [
      { "start_ms": 0, "end_ms": 2000, "text": "Welcome to TV7." },
      { "start_ms": 2000, "end_ms": 4500, "text": "The future of content creation." }
    ]
  }
  ```
- **Notes**:
  - Engine first extracts audio, then runs Whisper bindings.
  - If `timestamps=false`, the response omits `start_ms`/`end_ms` and returns plain text only.
  - If transcription is expected to be long-running, the handler returns `202 Accepted` with `job_id` and the result is retrieved via `/jobs/{id}`.
  - Cancelling a running job is not supported in v1; you can ignore the result and submit a new request.
- **Example cURL**:
  ```bash
  curl -X POST http://127.0.0.1:27107/audio/transcribe \
    -H "Content-Type: application/json" \
    -d '{"input_path":"/tmp/in.mp4","language":"auto","model":"base"}'
  ```

---

## LLM Domain

### Suggest Content
- **POST** `/llm/suggest`
- **Request**:
  - `task` (string, required): `title_generation` | `description_generation` | `clip_summary`.
  - `context` (string, required): Transcript or description text.
  - `options` (object, optional):
    - `count` (int, default 3): Max suggestions to return (<=10).
    - `creativity` (float 0.0-1.0, default 0.5): Maps to temperature.
    - `language` (string, optional): ISO-639-1 target language.
  - Example:
  ```json
  {
    "task": "title_generation",
    "context": "A video about baking sourdough bread at home.",
    "options": {
      "count": 3,
      "creativity": 0.7
    }
  }
  ```
- **Response**: `200 OK`
  ```json
  {
    "status": "ok",
    "suggestions": [
      "How to Bake Sourdough: A Beginner's Guide",
      "The Ultimate Sourdough Masterclass",
      "Baking Bread at Home: Easy & Delicious"
    ]
  }
  ```
- **Notes**:
  - For offline-only mode, the engine will refuse to call remote APIs; ensure models are available locally.
  - Mock mode always returns canned suggestions for deterministic tests.
  - Typically synchronous; if the prompt/model is heavy, the handler may return `202 Accepted` with `job_id` and you can poll `/jobs/{id}`.
- **Example cURL**:
  ```bash
  curl -X POST http://127.0.0.1:27107/llm/suggest \
    -H "Content-Type: application/json" \
    -d '{"task":"title_generation","context":"A video about baking sourdough bread at home.","options":{"count":3,"creativity":0.7}}'
  ```

---

## Planned/Optional Endpoints
- `/metrics` (GET): Prometheus metrics when built with the `metrics` feature (dev/QA only).
- `/jobs/{id}` (GET): Poll async job status (queued/running/completed/failed) and results/progress.

## Job Model (Async Operations)

### States
- `queued`: Accepted, waiting to start.
- `running`: In progress; may include `progress` 0.0–1.0 and an optional `message`.
- `completed`: Finished successfully with a `result` payload matching the endpoint schema.
- `failed`: Terminal failure with an `error` envelope.

### GET `/jobs/{id}`
- **Response**: `200 OK`
  ```json
  {
    "status": "running",
    "progress": 0.42,
    "message": "Trimming segment 1/3"
  }
  ```
  When finished:
  ```json
  {
    "status": "completed",
    "result": {
      "status": "ok",
      "output_path": "/abs/path/to/output.mp4",
      "duration_ms": 5000,
      "size_bytes": 1280000
    }
  }
  ```
- **Errors**:
  - `404` if job not found (expired/evicted).
  - `410` if job evicted due to TTL (future enhancement; initial impl may use `404`).
- **Notes**:
  - Jobs are stored in-memory with a short TTL; clients should fetch results promptly.
  - No cancellation in v1; submit a new request instead.
