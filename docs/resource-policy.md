# Resource policy

RunnerMesh will normalize policy **intent**, not claim that Windows and Linux expose identical controls. This is semantic portability, not fake equivalence.

## Planned intents

- **throughput** — favor CI progress when the workstation policy permits it.
- **foreground-friendly** — preserve interactive responsiveness.
- **constrained** — allow narrowly bounded background work.

An eligible node can be `FULL`, `THROTTLED`, `DRAINED`, or `OFFLINE`; these states communicate available capacity, not a promise of identical operating-system behavior.

## Backend direction

A future Windows backend may use Job Objects, process priority, CPU-rate controls where appropriate, memory controls where appropriate, and EcoQoS or power throttling where appropriate.

A future Linux backend may use cgroup v2, systemd scopes or slices, `nice`, and CPU, memory, I/O, or task controls.

The backend must report which capabilities it can actually provide. RunnerMesh will not present a generic cross-platform cgroup compatibility layer.

## Policy precedence

Manual policy wins over automatic sensing. Future automatic admission may consider user activity, CPU and memory pressure, power state, workload class, latency-sensitive activity, hysteresis, and cooldowns. When uncertain, it must preserve workstation use and refuse new CI admission.

No resource-control implementation is present yet.
