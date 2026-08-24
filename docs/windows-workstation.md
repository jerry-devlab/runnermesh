# Windows Workstation Mode

The first planned product path is GitHub Actions on Windows interactive workstations using the official GitHub self-hosted runner.

## Default execution model

Ordinary Workstation Mode runs under the intended interactive user session. It does not require `NETWORK SERVICE` or another service identity. Service/headless execution is planned as a distinct optional backend with its own design and security review.

## PowerShell contract

The Windows baseline is PowerShell 7. Installation method is intentionally not prescribed: a supported `pwsh` may come from a user-scoped packaged/MSIX installation or another supported installation model. The product requirement is simply that `pwsh` is resolvable and functional from the selected execution identity.

## Work-root ownership

One execution identity owns one active work root. The same identity may reuse that root. Different identities must not share an active work root, and ownership conflicts must not be papered over by globally changing Git safe-directory configuration.

## Admission and supervision direction

Future Windows behavior will observe runner lifecycle and local conditions, expose explicit modes and states, and perform graceful drain. Planned supervision includes launch, connected/listening detection, process-tree ownership, graceful stop, restart/reconnect, and work-root ownership.

No Windows service, daemon, runner installer, resource controller, or GitHub integration is implemented in this bootstrap.

## Networking

The official runner initiates outbound connectivity. A normal workstation does not need inbound public access, port forwarding, DDNS, or a VPN merely to run GitHub Actions. Future RunnerMesh control sessions are also intended to be outbound-initiated.
