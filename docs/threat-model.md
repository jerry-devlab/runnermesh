# Threat model

RunnerMesh is designed for persistent interactive workstations, where user experience and local trust boundaries matter as much as CI throughput.

## Primary properties

- The human owner has priority over CI work.
- Uncertainty fails open for normal workstation use and fails closed for new CI admission.
- Persistent trusted workstations must not execute arbitrary untrusted public-fork code by default.
- Manual policy choices override automatic sensing.
- CI source, logs, and artifacts remain with the CI provider rather than passing through a RunnerMesh proxy.
- Ordinary operation requires no inbound public workstation access.
- Separate execution identities must not actively share a work root.
- RunnerMesh must use minimal authority and avoid broad host-wide configuration changes.

## Trust boundaries

RunnerMesh distinguishes trusted self-hosted execution from hosted, disposable, or isolated execution. The boundary is explicit: a contributor must not assume that a persistent workstation is safe for all repository events merely because it can run CI.

Future privileged host changes require narrow scope, transactional behavior, durable receipts, reconciliation after interruption, and reversibility where practical. Loss of synchronous helper completion alone is not proof that a privileged transaction failed.

## Out of scope in this bootstrap

This repository does not yet implement admission, resource controls, a privileged helper, a broker, isolation, runner registration, or a secrets-management system. The absence of an implementation is not a claim that these concerns are solved.
