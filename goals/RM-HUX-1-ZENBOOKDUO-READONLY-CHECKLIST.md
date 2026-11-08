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

## Disposition mapping

| Result | Disposition |
|---|---|
| Counts > 0 with Ok/Incomplete honestly reported | HUX_1_NONEMPTY=PASS (host-specific) |
| candidate_count=0 with Ok | host has no runners; still not proof for other hosts |
| Unavailable disguised as Ok empty | FAIL |
