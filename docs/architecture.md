# Architecture

RunnerMesh is a capacity orchestration layer for interactive workstations. It contributes supply to a CI platform; it does not replace that platform's workflow scheduler or runner protocol.

## Boundaries

GitHub Actions remains responsible for workflow parsing, demand queues, job dependencies, runner assignment mechanics, logs, checks, artifacts, and the job protocol. RunnerMesh manages the availability and lifecycle of contributed execution capacity.

RunnerMesh does not proxy source code, CI logs, or artifacts. Those data flows remain on the CI-provider data plane. Its future control plane is limited to capacity, policy, capability, and lifecycle metadata.

## Conceptual layers

1. **CI demand** — GitHub Actions owns demand and queue semantics.
2. **Placement** — RunnerMesh determines eligible contributed capacity and capabilities.
3. **Admission** — A workstation decides whether it should accept new CI work now.
4. **Resource policy** — Native operating-system mechanisms decide how admitted work coexists with foreground use.

This separation keeps capacity policy explainable and avoids creating another generic scheduler.

## Human-first operation

The workstation owner has priority. RunnerMesh is planned to fail open for workstation use and fail closed for new CI admission: if policy or sensing becomes uncertain, the human retains normal use and the node does not accept additional work.

Manual modes override automatic sensing. Planned modes are `auto`, `work`, `gaming`, `idle`, `maintenance`, and `force-ci`; planned node states are `FULL`, `THROTTLED`, `DRAINED`, and `OFFLINE`.

## Capacity and lifecycle

Future placement is capability-aware. Future workstation supervision will cover launch, connected/listening detection, process-tree ownership, graceful stop, restart/reconnect, and work-root ownership. A drained node finishes eligible active work but accepts no new work.

The default Windows Workstation Mode is an ordinary intended user session:

```text
ordinary user session -> RunnerMesh -> official GitHub Actions runner
```

Service or headless execution is a separate future backend, not a requirement for ordinary workstation operation.

## Work-root contract

Each execution identity owns one active work root. The same identity can reuse its own work root; different identities must not share one while active. RunnerMesh must not resolve ownership conflicts by globally weakening Git safe-directory protections.

## Privileged operations

Future privileged host operations must be narrowly scoped, transactional, batched, reversible where practical, durably receipted, and reconcilable after control-plane interruption. A non-elevated control plane must not infer a transaction failed only because synchronous helper completion was lost.

No privileged helper is implemented in this bootstrap.
