# Engine Artifact Map

Default destination root (in this repo):

- `docs/contracts/tv7-axum-engine`

Suggested structure:

- `docs/contracts/tv7-axum-engine/openapi/engine-openapi.json`
- `docs/contracts/tv7-axum-engine/examples/*.json`
- `docs/contracts/tv7-axum-engine/compatibility/*.md`

Shared publication:

- Use `TV7_ENGINE_CONTRACT_DEST_ROOT=<shared-path>` to publish to shared contract trees when authorized.

Rules:

- Keep filenames stable across revisions.
- Add a changelog note in PR text when artifact shape changes.
- Link consumer repos that depend on these files.
