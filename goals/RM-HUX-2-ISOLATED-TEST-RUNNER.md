# HUX-2 isolated test Runner — Owner prep sheet

`PRODUCTION_MUTATION=false` until sealed trusted Owner authorization is issued.
This sheet is prepare-only. Do not paste private paths/IDs into public receipts.

## Isolation rules

- Use a **disposable** Windows Service Runner identity — never the production dogfood runner.
- Exact SCM service name only (no wildcards).
- Exact opaque `image_token` + `execution_identity_token` captured at prestate.
- Managed admission selector owned only by that runner.
- Separate Owner token from H2 cutover.

## Private Owner fields (offline)

```text
SERVICE_NAME=
IMAGE_TOKEN=          # from opaque_image_token(binary path)
EXEC_IDENTITY_TOKEN=  # from start account
RUNNER_HOME=
WORK_ROOT=
SCOPE=
SERVER_RUNNER_ID=
START_MODE_BEFORE=
STATE_BEFORE=
ADMISSION_SELECTOR_BEFORE=
```

## Acceptance prerequisites

```text
SYNTHETIC_PASS=required
SOURCE_READY=required
INDEPENDENT_SECURITY_REVIEW=PASS   # not Cursor self-review
TRUSTED_OWNER_AUTH=ISSUED          # UnissuedOwnerAuthorizationVerifier today
WORKER_BUSY=false
LIVE_NOT_AUTHORIZED until token window opens
```

## Stop conditions

Identity drift · Busy Worker · restore step fail · unexpected second service ·
UAC outside prepared helper · Owner abort · window expiry.
