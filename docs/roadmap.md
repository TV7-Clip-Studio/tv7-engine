# Rust Engine Roadmap (Core + Service)

## Phase 1: Workspace & Boundaries (Current)
- [ ] Initialize Cargo workspace with `engine-core`, `engine-service`, and stub `engine-ffi`.
- [ ] Define core traits and error types for video, audio, and LLM; provide mock implementations.
- [ ] Wire Axum/Tokio sidecar that delegates handlers to core traits (mock by default).
- [ ] Config + CLI (`config`, `clap`), logging/tracing, loopback-only binding, absolute-path validation.
- [ ] Job scaffold for long-running work with in-memory store and `/jobs/{id}`.
- [ ] CI: build/test (`cargo fmt`, `cargo clippy`, `cargo test`) and workspace linting.

**Definition of Done (P1)**: Workspace builds; mock handlers satisfy `docs/api_spec.md`; README documents how to run; core/service separation enforced by dependencies.

## Phase 2: Real Compute Modules
- [ ] Integrate ffmpeg (configurable path) and implement `RealVideoRenderer` in `engine-core`.
- [ ] Integrate whisper.cpp bindings for `RealTranscriber`; manage model paths and language autodetect.
- [ ] Integrate llama.cpp/local LLM for `RealLlmClient`; prompt templates + safety guards.
- [ ] Progress reporting from core up through job updates; atomic writes for outputs.
- [ ] Golden tests for trim/watermark, transcription timing, and deterministic LLM prompts where possible.

**Definition of Done (P2)**: Endpoints return real results on sample assets; progress visible via jobs; errors map to HTTP envelopes; tests cover the happy path and validation.

## Phase 3: Stabilization & Caching
- [ ] Model downloading/caching with quota checks and resume support.
- [ ] Work dir/cache eviction strategy for temp outputs and models.
- [ ] Observability: structured logs by default, optional `/metrics` behind feature flag.
- [ ] Error recovery, timeouts, and backpressure safeguards in the job system.
- [ ] Release packaging for macOS/Windows/Linux sidecar binaries.

**Definition of Done (P3)**: Stable downloads/caching, job system survives long runs, logs/metrics usable for support, signed binaries produced in CI.

## Phase 4: Mobile Exploration (FFI)
- [ ] Implement `engine-ffi` C ABI that exposes the same operations as the HTTP surface.
- [ ] Minimal Dart FFI bindings and smoke tests for iOS/Android (no Axum server on mobile).
- [ ] Parity checks to keep FFI + HTTP behaviors aligned.

**Definition of Done (P4)**: FFI library builds for iOS/Android targets, smoke tests execute core calls successfully, documented minimal binding examples for Flutter mobile.

## Phase 5: Platform Optimization & Hardening
- [ ] Metal acceleration (macOS), Vulkan/DirectML (Windows), optional CUDA paths.
- [ ] Performance profiling and memory ceilings; benchmarks for representative workloads.
- [ ] Crash reporting hook (opt-in) for the engine binary.
- [ ] Auto-update strategy (or clear handoff to Flutter updater).
- [ ] Security review: path traversal checks, sandboxing of temp dirs, job TTL and eviction policies.

**Definition of Done (P5)**: Perf targets documented, platform accelerations verified where available, security checklist cleared, updater/release story finalized.

## Cross-Cutting Backlog
- [ ] Developer onboarding docs and example assets for tests.
- [ ] CI matrix for macOS (Intel/ARM) and Windows.
- [ ] Job system limits: TTL eviction, max concurrency, crash/restart behavior documented and tested.
- [ ] Compatibility layer for per-domain mock/real selection to support partial testing.
- [ ] CLI utilities for debugging: path canonicalization, model cache inspection, job dump.
