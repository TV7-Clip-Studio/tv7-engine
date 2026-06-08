---
name: engine-local-diagnostics
description: Diagnose local tv7 axum engine failures across toolchain, runtime startup, port binding, and workspace state. Use when cargo checks fail, service boot fails, local HTTP calls fail, or environment drift causes inconsistent behavior.
metadata:
  short-description: Triage engine local runtime failures
---

# Engine Local Diagnostics

Find and fix local development failures in `tv7-axum-engine` quickly with deterministic checks.

## Workflow

1. Capture the exact failure surface.
   - Build failure, runtime crash, bind error, request failure, or test failure.
2. Validate local prerequisites.
   - Run `./.codex/skills/engine-local-diagnostics/scripts/check_local_tools.sh`.
3. Reproduce with narrow commands.
   - `cargo check`
   - `cargo test`
   - `cargo run -p engine-service`
4. Inspect likely fault domains.
   - Toolchain mismatch
   - Missing local dependency (for media tooling)
   - Config/env mismatch
   - Route/handler panic
5. Implement minimum viable fix and re-run checks.
   - Avoid broad refactors during triage.
6. Document root cause and confidence.
   - Capture command, error, fix, and remaining risk.

## Done Criteria

- Root cause identified with reproducible command.
- Fix is validated by targeted command plus `cargo check`.
- Residual risks and follow-up actions are explicit.
