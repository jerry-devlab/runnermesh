# HUX-1 ZenBookDuo read-only checklist

`PRODUCTION_MUTATION=false` · read-only SCM/process observation only.

The development host may report `candidate_count=0`. That result does **not**
represent ZenBookDuo or any other multi-runner workstation.

## Steps

1. Build the same Draft candidate binaries used for HUX foundation/productization.
2. On ZenBookDuo, run:
   ```text
   runnermesh-hux-sandbox discover-readonly
   ```
3. Record only privacy-safe fields:
   - candidate_count
   - observed_only_count / refused_count
   - identity_complete_count
   - service_enumeration / session_enumeration / process_observation
   - incomplete
4. Confirm incomplete/unavailable is never rewritten into a successful empty inventory.
5. Do not start/stop Services, change start mode, touch labels, or open credential files.
6. File private notes offline; public checkpoint may store counts only.

## Owner command card (ZenBookDuo)

```text
cargo build -p runnermesh --bin runnermesh-hux-sandbox
.\target\debug\runnermesh-hux-sandbox.exe discover-readonly
```

Expected privacy-safe stdout keys only. Paste counts into private notes; public
checkpoint may record `candidate_count` / `identity_complete_count` /
enumeration statuses.

Incomplete identity must stay refused / non-enrollable (covered by discovery
unit tests). Do not “fix” empty inventories.

## Implementer host RO sample (not ZenBookDuo)

Observed on the Implementer Windows workstation during
`RM-LRM-HUX2-READINESS-001` (read-only; no Service mutation):

```text
candidate_count=0
observed_only_count=0
refused_count=0
identity_complete_count=0
service_enumeration=Ok
session_enumeration=Ok
process_observation=Ok
incomplete=false
```

Disposition: local empty inventory with honest Ok channels.

```text
HUX1_NONEMPTY_LOCAL=EMPTY_OK
HUX1_NONEMPTY_ZENBOOKDUO=PENDING
```

Do not mark ZenBookDuo PASS until that host itself is observed.

## Privacy-safe result collection slot (awaiting ZenBookDuo data)

Fill only after running `discover-readonly` on ZenBookDuo. Leave blanks until then.
Public repo may store these fields only — never service names, paths, SIDs, or
runner IDs.

```text
HOST_LABEL=ZenBookDuo
OBSERVED_AT=
CANDIDATE_HEAD=
candidate_count=
observed_only_count=
refused_count=
identity_complete_count=
service_enumeration=
session_enumeration=
process_observation=
incomplete=
HUX1_NONEMPTY_ZENBOOKDUO=PENDING
PRODUCTION_MUTATION=false
```

## Disposition mapping

| Result | Disposition |
|---|---|
| Counts > 0 with Ok/Incomplete honestly reported | HUX_1_NONEMPTY=PASS (host-specific) |
| candidate_count=0 with Ok | host has no runners; still not proof for other hosts |
| Unavailable disguised as Ok empty | FAIL |
| ZenBookDuo data not yet supplied | HUX1_NONEMPTY_ZENBOOKDUO=PENDING |
