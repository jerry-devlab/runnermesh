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

RunnerMesh contributes managed execution capacity. It can eventually report capability and admission state, supervise the official runner in an approved execution mode, and drain capacity gracefully.

## Trust boundary

Trusted persistent self-hosted workstations are not the default place to execute arbitrary untrusted public-fork code. Such work should use GitHub-hosted, disposable, or appropriately isolated execution. Public project validation in this repository uses GitHub-hosted runners only.

## Data and networking boundary

The official runner initiates outbound connectivity; ordinary workstation participation needs no inbound public access. CI source, logs, and artifacts remain on the CI provider's data plane. A future RunnerMesh control plane should transport capacity, policy, and capability metadata only.

No GitHub API client, runner registration flow, or broker exists in this bootstrap.
