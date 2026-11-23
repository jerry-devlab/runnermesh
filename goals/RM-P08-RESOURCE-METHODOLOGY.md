# RunnerMesh Resource Measurement & Capability Methodology (P08)

`PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001` · `TRAIN_ID=RM-160H-T07-QUALIFY-RESOURCE-CLOSE` · `PRODUCTION_MUTATION=false`

This document defines the read-only resource observation methodology, attribution
boundaries, and capability detection rules for RunnerMesh.

---

## 1. Core Principles & Safety Invariants

1. **Read-Only Observation Only**:
   - RunnerMesh v0.1 NEVER creates Windows Job Objects.
   - RunnerMesh v0.1 NEVER attaches processes to containment objects.
   - RunnerMesh v0.1 NEVER modifies CPU affinity masks, priority classes, or EcoQoS power throttling states.
   - All containment and enforcement mechanisms are strictly reserved for future programs (v0.2+).

2. **No Fabricated Savings Invariant**:
   - Missing or unattributable telemetry MUST remain `UNKNOWN` / `None`.
   - Never represent absent or unmeasured consumption as `0 B` or `0% CPU`.
   - Never compute or advertise theoretical efficiency savings when real process tree attribution is unproven.

3. **Freshness & TTL Boundaries**:
   - Real-time samples carry an explicit timestamp (`sampled_at_unix_ms`) and validity TTL (`ttl_ms`, default 5,000 ms).
   - Samples exceeding the TTL automatically degrade to `ResourceSampleConfidence::Unknown`.
   - Stale data is never treated as active evidence.

---

## 2. Process Tree Attribution Model

Attribution of host memory and CPU utilization to an enrolled GitHub Runner follows four explicit dispositions:

| Disposition | Definition | Confidence Mapping |
|---|---|---|
| `DIRECT_VERIFIED` | The process PID, start account, and executable image exactly match the enrolled runner listener or official runner worker. | `ESTIMATE` (if fresh) |
| `CHILD_TREE` | Descendant process spawned directly under the verified runner tree during job execution. | `ESTIMATE` (if fresh) |
| `UNCERTAIN` | Ambiguous PID, process exited before query completion, cross-session execution, or unverified path. | `UNKNOWN` |
| `UNSUPPORTED` | Host query failed, access denied, or platform lacks query capability. | `UNKNOWN` |

When aggregating resources across multiple enrolled runners:
- If ANY runner in the evaluation set has an `UNKNOWN` or stale sample, the aggregate confidence is marked `UNKNOWN`.
- Aggregates show only verified totals alongside explicit unknown and stale counts.

---

## 3. Platform Capability Detection (Query-Only)

RunnerMesh probes the underlying platform to plan for future resource policies without mutating the system:

1. **Nested Job Objects**:
   - Windows 8 / Server 2012+ (NT 6.2+, Build >= 9200) natively supports nested Job Objects.
   - Detected via non-mutating OS build query (`RtlGetVersion`).

2. **EcoQoS / Power Throttling**:
   - Windows 10 Version 2004+ (Build >= 19041) and Windows 11 support EcoQoS execution speed throttling (`PROCESS_POWER_THROTTLING_EXECUTION_SPEED`).
   - Detected via OS build query without altering process token or state.

3. **Enforcement Gate**:
   - Invariant: `ResourcePolicyCapability.enforcement_authorized` is permanently `false` in v0.1.
   - Invariant: `ResourcePolicyCapability.query_only` is permanently `true`.

---

## 4. Future Enforcement Scope (v0.2+)

The following capabilities are explicitly deferred to future programs behind separate Owner gates:

- Attaching listener and worker process trees to dedicated containment Job Objects.
- Enforcing hard working-set memory limits or commit charges.
- Applying EcoQoS / background efficiency mode to idle runners.
- Modifying CPU affinity masks or core scheduling allocations.
