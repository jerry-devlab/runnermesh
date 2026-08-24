# G08 — Official Runner Observer

## Mission

Read and classify the local official GitHub Actions runner without controlling it.

## Deliver

Read-only discovery for configured runner home/metadata where safely available, Listener/Worker processes, execution identity, owned work-root evidence, `RunnerPhase`, and evidence-aware GitHub Actions `LinkState`.

Process existence alone is not `Connected`; insufficient evidence yields `Unknown`.

## Non-goals

No start/stop/drain/re-registration, no work-root mutation, no Organization settings, no service changes.

## Risk vector

Ordinary code + read-only runner observation.

## Gates

Synthetic fixtures + focused read-only trusted host proof where needed + hosted CI.

## Exit

Observer conservatively maps local evidence to stable snapshot fields without mutating runner state.

Next: G09.
