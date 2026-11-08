# Continuous source-train checkpoint

Privacy-safe recoverable status for the Cursor Implementer continuous train.
Does not grant production authority.

Separate lanes:

```text
LANE_A_V0_1=G15R_PASS / H2_WAITING_FOR_OWNER
LANE_B_V0_2_LRM=RUNTIME_INTEGRATION_CANDIDATE_DRAFT
```

```text
TRAIN=SOURCE_AND_SANDBOX_ONLY
PRODUCTION_MUTATION=false
AUTHORITATIVE_MAIN=ca1dd96e75a606b80d1978f83e973bd2586d36e4
NETWORK_STATUS=PROXY_OK_7890
CURSOR_SELF_REVIEW=PASS_LRM_RUNTIME_LOCAL_GATES
OWNER_GATE=required_for_h2_and_live_lrm
BLOCKERS=H2_OWNER_TOKEN; INDEPENDENT_SECURITY_ACCEPTANCE_FOR_H2_PENDING
INDEPENDENT_REVIEW=INDEPENDENT_REVIEW_PENDING_FOR_H2

# LRM lane
PR_STACK=main<-#51<-#52<-#53<-#54<-#55<-#56<-#57<-#58<-#59<-#61
LRM09_PR=59 HEAD=ed68ba7 CI=PASS DRAFT
LRM_RUNTIME_BRANCH=feat/lrm-runtime-integration
LRM_RUNTIME_BASE=ed68ba7 (#59 DRAINED ADR0004 alignment)
LRM_RUNTIME_LOCAL=PASS lib=220 clippy=-Dwarnings
LRM_RUNTIME_PR=61 DRAFT CI=REPAIR1_LINUX_UNUSED_AND_PUBLIC_AUDIT
LRM_REMAINING=hosted CI green for #61; live Service Control NOT_QUALIFIED; no force-merge onto v0.1

# Master train
ACTIVE_GOAL=RM-LONG-TRAIN-LRM-RUNTIME-001
T0=PASS DRAINED≠ListenerStopped; #60 independent security barrier preserved
T1=PASS FileRegistryStore wired into AgentCore + agent_runtime sandbox path
T2=PASS WindowsServiceDiscoverySource EnumServicesStatusEx read-only
T3=PASS Tray intent≠achieved labels; runtime discovery at startup
T4=IN_PROGRESS Draft #61; repair Linux unused + redact private paths

# v0.1 lane
G15R=PASS
H2_RC_READY=true
RC_COMMIT=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
RC_ARCHIVE_SHA256=b9e7e3ec19a999f4b62038e1c267e8c90517921c0fcb418c6c54a3c9f12bb571
RC_OPERATOR_HELPER_SHA256=78838d65c7432753ef63fbcda9e1e813b7fe0b739c15ad4423cf061f266958d9
RC_HOSTED_RUN=42452639359
H2_CUTOVER=WAITING_FOR_OWNER
H2_PLAN=goals/RM-V0_1-H2-OWNER-CUTOVER-PLAN.md
PR60=MERGED_TO_MAIN ca1dd96 (ledger+H2 plan only; not H2 authorization)

NEXT_SAFE_GOAL=CI_PASS_on_#61_OR_Owner_H2_review
STOP_BEFORE=production install; Service mutation; runner registration/label/credential changes; H2 execution; v0.1.0 publish; live LRM mutation
RECOVERY=isolated worktree branch feat/lrm-runtime-integration; PR=#61
```

Update this file only when decision-relevant train state changes; prefer binding
updates into the active implementation PR rather than opening a governance-only PR.
