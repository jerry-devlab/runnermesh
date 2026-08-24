# RunnerMesh contributor guidance

- Keep RunnerMesh Rust-first and the bootstrap intentionally compact.
- Preserve scope discipline; do not silently broaden authority, platform support, or product claims.
- Keep machine-readable JSON contracts stable once they are introduced.
- Never place private dogfood data in public code, fixtures, examples, documentation, or tests.
- Do not reimplement CI-provider responsibilities such as workflow parsing, queueing, job dependencies, logs, checks, artifacts, or runner protocol.
- Keep trusted self-hosted execution and untrusted-code execution as explicit, separately reviewed boundaries.
- Prefer documented policy intent over pretending operating systems have identical resource-control mechanisms.
