# GT-01 candidate gap map (privacy-safe)

`PRODUCTION_MUTATION=false` · SOURCE_AND_SANDBOX_ONLY

Bound to Draft PR #65 on `feat/160h-gt01-field-baseline` (stacked on PR #63 tip).
Exact HEAD is the tip of that branch after hosted CI; do not self-embed SHA here.

## Host truth

| Slot | Disposition | Notes |
| --- | --- | --- |
| HUX-0 tray visual | `PASS_OWNER_ACCEPTED` | Owner accepted; no re-run |
| HUX-1 development host RO | `EMPTY_OK` | Zero candidates, channels Ok |
| HUX-1 ZenBookDuo RO | `PENDING` | `operator-card-hux1` ready; not observed here |
| HUX-2 / HUX-3 live | `NOT_AUTHORIZED` | Outside GT-01 |

## Source package status (GT-01 executable)

| Area | Status | Notes |
| --- | --- | --- |
| P02 identity / discovery | **PASS_SOURCE** | Service/Session identity, quoting, enrollment |
| P04-05 FakeGithub matrix | **PASS_SOURCE** | lag/stale/permission/link-loss/race/scope-drift/racing-assignment |
| P05 multi-runner policy | **PASS_SOURCE** | hard-safety/Zen/Force-CI; A/B; restart refuse; stale PID |
| P06 UX/privacy/packaging | **PASS_SOURCE** | partial-batch, Tray i18n, CLI privacy, install/update sandbox |
| P01 G15R reverify | **PASS_READ_ONLY** | `fdd9fe6b…` + run `42452639359` still PASS; no rebuild |
| P03 synthetic≠live | **PASS_SOURCE** | forgeable grant cannot mutate gated Windows; Interrupted≠restore PASS |
| P04 scoped selector proposal | **PASS_SOURCE** | proposed namespaced selectors stay unapproved |

## Remaining (Owner / external)

1. HUX-1 ZenBookDuo collection — PENDING
2. Independent security review / H2 token / HUX-2 live / ADR 0006 / H3
3. GT-02 continues P01 Owner-packet polish without live mutation

## Non-goals this map

Live SCM mutation, label changes, H2 cutover, independent security acceptance,
production install, and merging the Draft stack into frozen v0.1 RC.
