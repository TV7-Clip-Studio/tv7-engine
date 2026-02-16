---
name: engine-shared-artifact-publish
description: Publish tv7 axum engine contract and compatibility artifacts into canonical contract locations. Use when engine API schemas, request-response examples, or compatibility notes change and must be prepared for cross-repo consumers.
metadata:
  short-description: Upload engine artifacts to shared docs
---

# Engine Shared Artifact Publish

Update shared artifact files used by other TV7 repos after engine contract changes.

## Workflow

1. Identify source artifact(s) from engine work.
   - Typical sources: generated API schemas, request/response examples, compatibility notes.
2. Choose destination root.
   - Default destination root: `docs/contracts/tv7-axum-engine` (in-repo staging).
   - Optional override via `TV7_ENGINE_CONTRACT_DEST_ROOT` for shared publication paths.
   - Keep filenames stable to reduce consumer churn.
3. Publish artifacts with the helper script.
   - Run `./.codex/skills/engine-shared-artifact-publish/scripts/publish_shared_artifacts.sh <source-path> [artifact-name]`.
4. Record traceability.
   - Keep hash output from the script in your PR notes.
   - Link producer and consumer repos when contract shape changed.
5. Validate policy alignment.
   - Confirm cross-repo policy expectations are met for producer/consumer order.
   - If compatibility is not backward-safe, ensure producer lands first.

## Bundled Resources

- `references/artifact-map.md` for canonical destination guidance.
- `scripts/publish_shared_artifacts.sh` for deterministic copy + hash output.

## Done Criteria

- Shared artifacts are copied to the intended destination root.
- Hashes are captured for review traceability.
- Cross-repo notes identify impacted consumers.
