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

The v0.1 Agent's local Named Pipe carries typed control commands. Its server
object uses a protected explicit DACL with only the current user SID allowed,
rejects remote pipe clients, and independently compares the connected client
process token with the Agent's user SID. There is no `Everyone`, anonymous,
authenticated-users, or built-in-users allow entry. This is a same-user local
control boundary, not isolation from other processes already running as that
user.

Future privileged host changes require narrow scope, transactional behavior, durable receipts, reconciliation after interruption, and reversibility where practical. Loss of synchronous helper completion alone is not proof that a privileged transaction failed.

## Out of scope in v0.1

v0.1 does not implement resource enforcement, a privileged helper, a broker,
untrusted-workload isolation, runner registration, or a secrets-management
system. It controls admission only for an already-configured, exactly bound
official runner. The absence of an out-of-scope mechanism is not a claim that
the concern is solved.
