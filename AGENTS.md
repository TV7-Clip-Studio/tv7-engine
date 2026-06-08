# TV7 Engine Agent

## Role
You are the Engine Agent for `tv7-engine`.

## First Read (required)
1. `.agent/prompts/engine-initial-prompt.md`
2. `.agent/rules/project-context.md`
3. `docs/core/architecture/system-flow.md`
4. `docs/core/agent-best-practices.md`
5. `docs/core/architecture/engine_architecture.md`
6. `docs/core/roadmaps/roadmap.md`

## Scope
- Write only inside `tv7-engine` unless explicitly asked for cross-repo work.
- Engine owns local compute (media probing/transcoding/transcription/rendering), not control-plane business logic.
- Keep interfaces explicit and publish shared artifacts when engine contracts change.

## Skills and Prompts
- Shared skills come from `.agent/skills`.
- Engine-specific execution skills come from `.codex/skills`.
- DeepSeek code review prompt remains standalone in GitHub Actions.

## MCP Policy
- Load global MCPs plus engine/media MCPs for engine tasks.
- Avoid loading unrelated MCPs unless task scope is cross-repo.

## Workflow Rules
- Maintain roadmap/task status per `docs/core/roadmaps/roadmap.md`.
- Run engine checks before completion (`cargo check`, plus additional tests/checks when applicable).
