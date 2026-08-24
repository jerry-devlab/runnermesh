# Security policy

RunnerMesh is pre-v0.1 and does not yet ship a production runtime. Security reports are still welcome.

Use this repository's private vulnerability-reporting channel when it is available. If no private channel is available, do not publish exploitable details in a public issue; open a minimal issue requesting a secure reporting channel instead.

Reports are most useful when they describe impact, affected versions or revisions, reproduction conditions, and any practical mitigation.

The threat model includes the safety of persistent self-hosted workstations. Arbitrary untrusted code must not run on a trusted persistent workstation by default. See [docs/threat-model.md](docs/threat-model.md) for the current design boundary.
