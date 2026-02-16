# Probe Checklist

Use this list when reviewing probe implementations:

- Input path validation is strict (absolute path, non-empty).
- Probe output includes stable fields used by downstream clients.
- Errors preserve root cause context and are human-readable.
- Tests cover happy path and at least three error classes.
- Service-layer mapping from probe errors to HTTP status is explicit.
