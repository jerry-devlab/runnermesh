# RunnerMesh Project Charter

RunnerMesh is a human-first CI capacity orchestration layer for interactive developer workstations.

## Product thesis

A workstation being online does not mean it is available for CI. RunnerMesh contributes only capacity that the human owner can safely spare.

## v0.1 definition

v0.1 is the first usable Windows single-workstation release. It supervises an already-configured official GitHub Actions runner from the intended ordinary user session, exposes Tray + CLI control, supports manual modes, Zen, Auto Lite probes, graceful drain, recovery, and production-safe install/update/rollback.

## Permanent boundaries

- GitHub Actions owns workflow demand, queues, protocol, logs, checks, and artifacts.
- RunnerMesh manages contributed supply/admission/lifecycle.
- Human use has priority.
- Fail open for workstation use; fail closed for CI admission.
- One execution identity owns one active work root.
- Public untrusted fork code does not run on persistent personal workstations by default.
- Privileged host mutations are narrow, transactional, durably receipted, and reconciled.
- Product code is Rust-first; native OS APIs are used where appropriate.
- Semantic portability is preferred over fake cross-platform equivalence.

## Development philosophy

Use evidence-first, risk-based development. Keep Goals focused and independently revertible. Validate changed risk once, reuse accepted unchanged-risk evidence, and stop unattended trains at real-machine, trust, and publication boundaries.
