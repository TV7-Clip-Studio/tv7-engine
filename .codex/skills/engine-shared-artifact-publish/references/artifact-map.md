# Engine Artifact Map

Default destination root (canonical Core contracts through the repo symlink):

- `docs/core/contracts/tv7-axum-engine`

Suggested structure:

- `docs/core/contracts/tv7-axum-engine/openapi/engine-openapi.json`
- `docs/core/contracts/tv7-axum-engine/examples/*.json`
- `docs/core/contracts/tv7-axum-engine/compatibility/*.md`

Shared publication:

- Use `TV7_ENGINE_CONTRACT_DEST_ROOT=<shared-path>` only for nonstandard shared contract destinations.

Rules:

- Keep filenames stable across revisions.
- Add a changelog note in PR text when artifact shape changes.
- Link consumer repos that depend on these files.
