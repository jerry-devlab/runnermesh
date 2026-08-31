# GitHub Actions integration boundary

RunnerMesh's first planned integration target is GitHub Actions with the official GitHub self-hosted runner. RunnerMesh does not reimplement the runner protocol.

## Provider-owned responsibilities

GitHub Actions remains responsible for:

- workflow parsing;
- queueing and demand semantics;
- job dependencies;
- runner assignment mechanics;
- logs and checks;
- artifacts; and
- source and job protocol.

RunnerMesh contributes managed execution capacity. v0.1 models capability and
admission state, observes the exact official runner, and withdraws capacity by
removing one reserved GitHub-native label. It does not implement workflow or
runner-protocol behavior.

## Managed-capacity workflow contract

Jobs intended to use RunnerMesh-managed capacity include the canonical custom
selector in addition to their ordinary self-hosted platform labels:

```yaml
runs-on:
  - self-hosted
  - Windows
  - X64
  - runnermesh-admit
```

GitHub evaluates these labels cumulatively. Jobs that omit
`runnermesh-admit` remain outside RunnerMesh's withdrawal guarantee.

## Narrow admission authority

The admission backend binds one organization or repository scope, one exact
runner ID and name, and the canonical reserved label `runnermesh-admit`. Its
public operations are deliberately limited to:

- observe exact runner labels;
- add the reserved label; and
- remove the reserved label.

Every mutation is followed by observation/readback. RunnerMesh refuses
ambiguous runner identity, a same-name selector on another runner, and reserved
label ownership drift. It never replaces or deletes all labels, touches an
unrelated custom label or runner, changes runner groups, or changes runner
registration through this boundary.

The least documented fine-grained authority is organization `Self-hosted
runners: write` or repository `Administration: write`, restricted to the
configured scope. Normal configuration contains an opaque provider/key
reference only. Credential material is acquired at request time through a
provider boundary, kept out of serialization and debug output, and zeroed when
the short-lived lease is dropped. The Windows source adapter resolves generic
credentials from Windows Credential Manager and gives the bytes directly to a
short-lived lease. Configuration and status contain only the opaque provider
and key reference. This source Goal does not provision or resolve a real Owner
credential.

Authentication failure, unavailable credentials, API unavailability, timeout,
and rate limiting remain visible reason-coded states. Transient failures use
bounded backoff. They never become `DRAINED` and never fall back to a local
Worker signal or kill.

## Two-phase withdrawal

Local policy intent establishes desired `DRAINED` immediately. Achieved
`DRAINED` is separate and requires reserved-selector absence, no exact bound
Worker, and consistent evidence. Label mutation/readback is not claimed as a
globally linearizable scheduler barrier. Work observed around that boundary is
represented conservatively as in-flight and may complete naturally.

## Trust boundary

Trusted persistent self-hosted workstations are not the default place to execute arbitrary untrusted public-fork code. Such work should use GitHub-hosted, disposable, or appropriately isolated execution. Public project validation in this repository uses GitHub-hosted runners only.

## Data and networking boundary

The official runner initiates outbound connectivity; ordinary workstation participation needs no inbound public access. CI source, logs, and artifacts remain on the CI provider's data plane. A future RunnerMesh control plane should transport capacity, policy, and capability metadata only.

The source tree contains both the typed GitHub REST boundary and a production
WinHTTP adapter. The transport fixes the authority to `api.github.com:443`,
keeps default TLS certificate verification enabled, refuses redirects, bounds
timeouts and response sizes, validates selected response headers, and keeps
the authorization value out of request debug state. Its operations remain the
existing exact-runner reads plus add-one/remove-one reserved-label requests;
positive readback is still mandatory in the admission backend.

The live source is deliberately inert without an Owner-supplied opaque
credential reference and exact binding. Tests inject fake wire and credential
providers, make no network calls, and use only synthetic identities. This Goal
does not configure the adapter, contact the GitHub API with Owner authority,
mutate a label, register a runner, or introduce a broker.
