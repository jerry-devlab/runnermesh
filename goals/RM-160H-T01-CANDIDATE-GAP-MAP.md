# GT-01 candidate gap map (privacy-safe)

`PRODUCTION_MUTATION=false` · SOURCE_AND_SANDBOX_ONLY

Bound to Draft PR #65 on `feat/160h-gt01-field-baseline` (stacked on PR #63 tip).
Exact HEAD is the tip of that branch after hosted CI; do not self-embed SHA here.

## Host truth

| Slot | Disposition | Notes |
| --- | --- | --- |
| HUX-0 tray visual | `PASS_OWNER_ACCEPTED` | Owner accepted; no re-run |
| HUX-1 development host RO | `EMPTY_OK` | Zero candidates, channels Ok |
| HUX-1 ZenBookDuo RO | `PENDING` | Operator card ready; not observed here |
| HUX-2 / HUX-3 live | `NOT_AUTHORIZED` | Outside GT-01 |

## P02 source gaps vs checklist

| Leaf | Status | Evidence |
| --- | --- | --- |
| P02-01 SCM config / ImagePath quoting / start-account hash | **IN_PROGRESS** | Path quoting fixed + unit tests; QueryServiceConfigW aligned; missing account → no forged identity |
| P02-02 path-boundary / bin layout / owner match | **PASS_SOURCE** | Sibling-prefix rejection + Listener/Worker bin suffix tests |
| P02-03 Service vs Session vs scheduled-task | **PASS_SOURCE** | Inventory refusals + unsupported observation-only |
| P02-04 owned Session prepare/adopt/stop plans | **PASS_SOURCE** | Synthetic backend refuses Busy/Unknown/foreign; live process I/O Owner-gated |
| P02-05 registry enroll collisions / no auto-enroll | **PASS_SOURCE** | Registry + discover view enrollment_eligible=false on refuse |
| P02-06 mixed-platform / nonempty RO host | **PENDING** | Ubuntu+Windows CI; ZenBookDuo nonempty still PENDING |

## Remaining GT-01 / early GT-02 executable work

1. HUX-1 ZenBookDuo collection remains PENDING — do not invent PASS.
2. P04-05 remaining: racing assignment + scope-drift FakeGithub cases.
3. P05 controller wiring beyond current synthetic matrix (Owner-gated live still closed).
4. Independent security review / Owner tokens remain external gates.

## Non-goals this map

Live SCM mutation, label changes, H2 cutover, independent security acceptance,
production install, and merging the Draft stack into frozen v0.1 RC.
