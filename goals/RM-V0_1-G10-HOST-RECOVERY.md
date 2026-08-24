# G10 — Host Observation and Recovery

## Mission

Complete read-only host sensing and Agent restart/reconstruction behavior before real runner mutation.

## Deliver

- CPU/memory/user-idle/session observations;
- snapshot health/reason integration;
- reconstruct runtime observations after Agent restart;
- safe existing-listener adoption decision using identity/home/work-root evidence;
- interruption/reconnect tests;
- sleep/resume-safe direction where practical.

## Non-goals

No production runner mutation, autostart activation, install/update activation, or resource throttling.

## Risk vector

Host read-only observation + recovery/concurrency code.

## Gates

Synthetic restart/adoption fixtures, read-only Windows observation proof where needed, hosted CI.

## Exit

Agent restart can reconstruct state and decide whether existing runner state is safely adoptable.

Next: G11 Human Gate H1.
