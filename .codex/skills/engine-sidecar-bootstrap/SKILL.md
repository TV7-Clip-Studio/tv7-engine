---
name: engine-sidecar-bootstrap
description: Bootstrap and harden the tv7 axum engine sidecar with loopback-only Axum wiring, config defaults, and startup validation. Use when creating or refactoring engine-service startup, routes, config loading, and health endpoints.
metadata:
  short-description: Bootstrap the local engine sidecar
---

# Engine Sidecar Bootstrap

Set up a reliable local-first sidecar in `engine-service` that delegates compute to `engine-core`.

## Workflow

1. Confirm architecture boundaries before editing.
   - Engine is local compute only, no account/billing/scheduling business logic.
   - Service must bind loopback (`127.0.0.1`) and accept absolute path inputs only.
2. Build the startup surface first.
   - Add app config (`port`, `work_dir`, `log_level`) with sane defaults.
   - Validate config at startup and fail fast with explicit errors.
3. Add a minimal HTTP contract.
   - Add `GET /health` and one engine operation route.
   - Keep handlers thin; delegate to `engine-core` traits/services.
4. Add logging and tracing.
   - Initialize `tracing_subscriber` with env filter support.
   - Log startup, bind address, and request failures with context.
5. Add safety guardrails.
   - Reject non-loopback binds by default.
   - Enforce absolute path checks at request boundary.
6. Validate and iterate.
   - Run `./.codex/skills/engine-sidecar-bootstrap/scripts/run_quality_gate.sh`.
   - If check failures are unrelated, report and continue with scoped fixes only.

## Done Criteria

- Service startup is deterministic and config-driven.
- Loopback binding and input validation rules are enforced.
- `cargo check`, `cargo test`, and `cargo clippy` pass.

