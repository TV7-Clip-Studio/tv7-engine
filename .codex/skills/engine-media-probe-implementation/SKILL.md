---
name: engine-media-probe-implementation
description: Implement and harden media probing in engine-core with typed errors, trait boundaries, and deterministic tests. Use when replacing probe stubs, integrating ffprobe metadata extraction, or shaping probe results consumed by engine-service.
metadata:
  short-description: Build and validate media probe logic
---

# Engine Media Probe Implementation

Replace placeholder probe logic in `engine-core` with production-safe media inspection behavior.

## Workflow

1. Define the probe contract first.
   - Keep output structs stable and explicit.
   - Use typed error enums for file-not-found, unsupported-format, and probe-tool failures.
2. Implement probe service behind traits.
   - Keep probe execution isolated from transport concerns.
   - Prefer dependency injection so service and future FFI can share logic.
3. Validate absolute-path and local-file assumptions.
   - Reject empty or relative paths early.
   - Return actionable errors for invalid inputs.
4. Add deterministic tests.
   - Add pure unit tests for validation and error mapping.
   - Add fixture-based tests for sample media where available.
5. Align service integration.
   - Ensure engine-service handlers map probe errors to stable HTTP responses.
6. Run verification.
   - Run `./.codex/skills/engine-media-probe-implementation/scripts/run_probe_checks.sh`.

## Done Criteria

- Probe implementation no longer uses hardcoded stub metadata.
- Errors are typed and mapped consistently.
- Engine core and service checks pass.

