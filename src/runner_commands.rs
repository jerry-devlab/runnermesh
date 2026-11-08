//! Typed multi-runner command contract (LRM-03).
//!
//! Read-only queries never mutate the registry. Mutating commands honor
//! `prepare_only` by returning a plan without applying intent. Applying intent
//! still never starts/stops a Windows Service or touches GitHub registration.

use serde::{Deserialize, Serialize};

use crate::{
    aggregate_runner_memory, DiscoveryReport, EnrollmentState, ReasonCode, RegistryError,
    RunnerMemorySample, RunnerRecord, RunnerRegistry, RunnerRegistryEditor,
    RunnerResourceAggregate,
};

/// Presentation vocabulary for Tray/CLI. Stable non-localized values.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunnerPresentationStatus {
    Enabled,
    Disabled,
    Draining,
    Busy,
    Stopped,
    Unknown,
    Refused,
}

/// Stable machine-facing summary of one registry record for IPC/CLI JSON.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerRecordView {
    pub local_runner_key: String,
    pub runner_id: u64,
    pub expected_runner_name: String,
    pub desired_enabled: bool,
    pub enrollment_state: EnrollmentState,
    pub managed_admission: bool,
    pub backend_kind: String,
    #[serde(default = "default_presentation_status")]
    pub presentation_status: RunnerPresentationStatus,
    #[serde(default)]
    pub memory_samples: Vec<RunnerMemorySample>,
    #[serde(default)]
    pub item_applied: Option<bool>,
    #[serde(default)]
    pub item_reason_code: Option<ReasonCode>,
}

fn default_presentation_status() -> RunnerPresentationStatus {
    RunnerPresentationStatus::Unknown
}

impl From<&RunnerRecord> for RunnerRecordView {
    fn from(record: &RunnerRecord) -> Self {
        Self {
            local_runner_key: record.local_runner_key.clone(),
            runner_id: record.runner_id,
            expected_runner_name: record.expected_runner_name.clone(),
            desired_enabled: record.desired_enabled,
            enrollment_state: record.enrollment_state,
            managed_admission: record.managed_admission,
            backend_kind: match &record.backend {
                crate::RegistryBackendBinding::WindowsService { .. } => {
                    "windows-service".to_owned()
                }
                crate::RegistryBackendBinding::UserSession => "user-session".to_owned(),
            },
            presentation_status: presentation_status_for(record),
            memory_samples: Vec::new(),
            item_applied: None,
            item_reason_code: None,
        }
    }
}

fn presentation_status_for(record: &RunnerRecord) -> RunnerPresentationStatus {
    match record.enrollment_state {
        EnrollmentState::Forgotten => RunnerPresentationStatus::Stopped,
        EnrollmentState::Enrolled if record.desired_enabled => RunnerPresentationStatus::Enabled,
        EnrollmentState::Enrolled => RunnerPresentationStatus::Disabled,
    }
}

/// Privacy-safe discovery candidate summary for Tray/CLI (no paths/IDs leaked).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveredCandidateView {
    pub local_discovery_key: String,
    pub backend_kind: String,
    pub disposition: String,
    #[serde(default)]
    pub refusal_reason: Option<String>,
    pub enrollment_eligible: bool,
    pub same_scope_collision: bool,
}

/// Authoritative runners command report shared by CLI/IPC.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerCommandReport {
    pub schema_version: u32,
    pub read_only: bool,
    pub prepare_only: bool,
    pub applied: bool,
    pub reason_code: Option<ReasonCode>,
    pub records: Vec<RunnerRecordView>,
    pub discovered_keys: Vec<String>,
    pub same_scope_collisions: usize,
    #[serde(default)]
    pub resource_aggregate: Option<RunnerResourceAggregate>,
    #[serde(default)]
    pub partial_failure: bool,
    /// Optional privacy-safe discovery rows (present on discover responses).
    #[serde(default)]
    pub discovered_candidates: Vec<DiscoveredCandidateView>,
}

impl RunnerCommandReport {
    fn from_registry(
        registry: &RunnerRegistry,
        read_only: bool,
        prepare_only: bool,
        applied: bool,
        reason_code: Option<ReasonCode>,
        discovered_keys: Vec<String>,
        same_scope_collisions: usize,
    ) -> Self {
        let records: Vec<RunnerRecordView> = registry
            .records
            .iter()
            .map(RunnerRecordView::from)
            .collect();
        Self {
            schema_version: crate::RUNNER_REGISTRY_SCHEMA_VERSION,
            read_only,
            prepare_only,
            applied,
            reason_code,
            records,
            discovered_keys,
            same_scope_collisions,
            resource_aggregate: None,
            partial_failure: false,
            discovered_candidates: Vec::new(),
        }
    }

    /// Attaches synthetic/timestamped resource samples for presentation.
    pub fn with_resource_samples(
        mut self,
        samples_by_key: &[(String, Vec<RunnerMemorySample>)],
        sampled_at_unix_ms: u64,
    ) -> Self {
        let mut flat = Vec::new();
        for record in &mut self.records {
            if let Some((_, samples)) = samples_by_key
                .iter()
                .find(|(key, _)| key == &record.local_runner_key)
            {
                record.memory_samples = samples.clone();
                flat.extend(samples.iter().cloned());
            }
        }
        self.resource_aggregate = Some(aggregate_runner_memory(&flat, sampled_at_unix_ms));
        self
    }

    /// Marks per-item outcomes for batch-style presentation (partial failure).
    pub fn with_item_outcome(
        mut self,
        local_runner_key: &str,
        applied: bool,
        reason: Option<ReasonCode>,
    ) -> Self {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|item| item.local_runner_key == local_runner_key)
        {
            record.item_applied = Some(applied);
            record.item_reason_code = reason;
        }
        self.partial_failure = self
            .records
            .iter()
            .any(|item| item.item_applied == Some(false))
            && self
                .records
                .iter()
                .any(|item| item.item_applied == Some(true));
        if self.partial_failure && self.reason_code.is_none() {
            self.reason_code =
                Some(ReasonCode::new("runners-partial-failure".to_owned()).expect("static reason"));
        }
        self
    }
}

/// Applies a runners-family Agent command against registry (+ optional discovery).
pub fn handle_runners_command(
    editor: &mut RunnerRegistryEditor,
    discovery: Option<&DiscoveryReport>,
    command: &crate::AgentCommand,
) -> Result<RunnerCommandReport, RegistryError> {
    match command {
        crate::AgentCommand::RunnersList => Ok(RunnerCommandReport::from_registry(
            editor.registry(),
            true,
            false,
            false,
            None,
            Vec::new(),
            0,
        )),
        crate::AgentCommand::RunnersDiscover => {
            let report = discovery.cloned().unwrap_or_else(DiscoveryReport::empty_ok);
            Ok(RunnerCommandReport {
                schema_version: crate::RUNNER_REGISTRY_SCHEMA_VERSION,
                read_only: true,
                prepare_only: false,
                applied: false,
                reason_code: if report.is_incomplete() {
                    ReasonCode::new("discovery-incomplete").ok()
                } else {
                    None
                },
                records: editor
                    .registry()
                    .records
                    .iter()
                    .map(RunnerRecordView::from)
                    .collect(),
                discovered_keys: report
                    .candidates
                    .iter()
                    .map(|candidate| candidate.local_discovery_key.clone())
                    .collect(),
                same_scope_collisions: report
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.same_scope_collision)
                    .count(),
                resource_aggregate: None,
                // Unavailable/Incomplete host evidence is a partial observation,
                // never a clean empty success.
                partial_failure: report.is_incomplete(),
                discovered_candidates: report
                    .candidates
                    .iter()
                    .map(discovered_candidate_view)
                    .collect(),
            })
        }
        crate::AgentCommand::RunnersInspect { local_runner_key } => {
            let registry = editor.registry();
            let Some(_) = registry.get(local_runner_key) else {
                return Err(RegistryError::NotFound);
            };
            let view = registry
                .records
                .iter()
                .filter(|record| record.local_runner_key == *local_runner_key)
                .map(RunnerRecordView::from)
                .collect();
            Ok(RunnerCommandReport {
                schema_version: crate::RUNNER_REGISTRY_SCHEMA_VERSION,
                read_only: true,
                prepare_only: false,
                applied: false,
                reason_code: None,
                records: view,
                discovered_keys: Vec::new(),
                same_scope_collisions: 0,
                resource_aggregate: None,
                partial_failure: false,
                discovered_candidates: Vec::new(),
            })
        }
        crate::AgentCommand::RunnersStatus { local_runner_key } => {
            let registry = editor.registry();
            let records = match local_runner_key {
                Some(key) => registry
                    .get(key)
                    .map(|record| vec![RunnerRecordView::from(record)])
                    .ok_or(RegistryError::NotFound)?,
                None => registry
                    .records
                    .iter()
                    .map(RunnerRecordView::from)
                    .collect(),
            };
            Ok(RunnerCommandReport {
                schema_version: crate::RUNNER_REGISTRY_SCHEMA_VERSION,
                read_only: true,
                prepare_only: false,
                applied: false,
                reason_code: None,
                records,
                discovered_keys: Vec::new(),
                same_scope_collisions: 0,
                resource_aggregate: None,
                partial_failure: false,
                discovered_candidates: Vec::new(),
            })
        }
        crate::AgentCommand::RunnersEnroll {
            local_runner_key,
            prepare_only,
        } => {
            let candidate = discovery
                .and_then(|report| {
                    report
                        .candidates
                        .iter()
                        .find(|candidate| candidate.local_discovery_key == *local_runner_key)
                })
                .ok_or(RegistryError::NotFound)?;
            if *prepare_only {
                if !candidate.capabilities.enrollment_eligible {
                    return Err(RegistryError::NotEnrollmentEligible);
                }
                return Ok(RunnerCommandReport::from_registry(
                    editor.registry(),
                    false,
                    true,
                    false,
                    Some(static_reason("runners-enroll-prepared")),
                    vec![local_runner_key.clone()],
                    usize::from(candidate.same_scope_collision),
                ));
            }
            editor.enroll_from_discovery(candidate, None, false)?;
            Ok(RunnerCommandReport::from_registry(
                editor.registry(),
                false,
                false,
                true,
                Some(static_reason("runners-enrolled")),
                vec![local_runner_key.clone()],
                0,
            ))
        }
        crate::AgentCommand::RunnersEnable {
            local_runner_key,
            prepare_only,
        } => mutate_enabled(
            editor,
            local_runner_key,
            true,
            *prepare_only,
            "runners-enable",
        ),
        crate::AgentCommand::RunnersDisable {
            local_runner_key,
            prepare_only,
            ..
        } => mutate_enabled(
            editor,
            local_runner_key,
            false,
            *prepare_only,
            "runners-disable",
        ),
        crate::AgentCommand::RunnersReconcile {
            local_runner_key,
            prepare_only,
        } => {
            let _ = editor
                .registry()
                .get(local_runner_key)
                .ok_or(RegistryError::NotFound)?;
            Ok(RunnerCommandReport::from_registry(
                editor.registry(),
                false,
                *prepare_only,
                !prepare_only,
                Some(static_reason(if *prepare_only {
                    "runners-reconcile-prepared"
                } else {
                    // Intent-only reconcile in LRM-03: no SCM/GitHub mutation.
                    "runners-reconcile-intent-only"
                })),
                vec![local_runner_key.clone()],
                0,
            ))
        }
        crate::AgentCommand::RunnersForget {
            local_runner_key,
            prepare_only,
        } => {
            if *prepare_only {
                let _ = editor
                    .registry()
                    .get(local_runner_key)
                    .ok_or(RegistryError::NotFound)?;
                return Ok(RunnerCommandReport::from_registry(
                    editor.registry(),
                    false,
                    true,
                    false,
                    Some(static_reason("runners-forget-prepared")),
                    vec![local_runner_key.clone()],
                    0,
                ));
            }
            editor.forget(local_runner_key)?;
            Ok(RunnerCommandReport::from_registry(
                editor.registry(),
                false,
                false,
                true,
                Some(static_reason("runners-forgotten")),
                vec![local_runner_key.clone()],
                0,
            ))
        }
        _ => Err(RegistryError::InvalidRecord("not a runners command")),
    }
}

fn mutate_enabled(
    editor: &mut RunnerRegistryEditor,
    local_runner_key: &str,
    desired_enabled: bool,
    prepare_only: bool,
    reason_prefix: &str,
) -> Result<RunnerCommandReport, RegistryError> {
    let _ = editor
        .registry()
        .get(local_runner_key)
        .ok_or(RegistryError::NotFound)?;
    if prepare_only {
        return Ok(RunnerCommandReport::from_registry(
            editor.registry(),
            false,
            true,
            false,
            Some(static_reason(&format!("{reason_prefix}-prepared"))),
            vec![local_runner_key.to_owned()],
            0,
        ));
    }
    editor.set_desired_enabled(local_runner_key, desired_enabled)?;
    Ok(RunnerCommandReport::from_registry(
        editor.registry(),
        false,
        false,
        true,
        Some(static_reason(&format!("{reason_prefix}-applied"))),
        vec![local_runner_key.to_owned()],
        0,
    ))
}

fn static_reason(code: &str) -> ReasonCode {
    ReasonCode::new(code.to_owned()).expect("runners reason codes are static kebab tokens")
}

fn discovered_candidate_view(candidate: &crate::DiscoveredRunner) -> DiscoveredCandidateView {
    DiscoveredCandidateView {
        local_discovery_key: candidate.local_discovery_key.clone(),
        backend_kind: match candidate.backend {
            crate::RunnerBackendKind::WindowsService => "windows-service".to_owned(),
            crate::RunnerBackendKind::UserSession => "user-session".to_owned(),
            crate::RunnerBackendKind::ScheduledTask => "scheduled-task".to_owned(),
            crate::RunnerBackendKind::Unknown => "unknown".to_owned(),
        },
        disposition: match candidate.disposition {
            crate::DiscoveryDisposition::ObservedOnly => "OBSERVED_ONLY".to_owned(),
            crate::DiscoveryDisposition::Refused => "REFUSED".to_owned(),
        },
        refusal_reason: candidate.refusal.map(|reason| reason.as_str().to_owned()),
        enrollment_eligible: candidate.capabilities.enrollment_eligible,
        same_scope_collision: candidate.same_scope_collision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AgentCommand, DiscoveredLifecycle, DiscoveredRunner, DiscoveredRunnerIdentity,
        DiscoveryCapabilities, DiscoveryDisposition, DiscoveryReport, OpaqueIdentityReference,
        RegistrationScope, RunnerBackendKind, WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
    };
    use std::path::PathBuf;

    fn identity() -> OpaqueIdentityReference {
        OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap()
    }

    fn discovery_report() -> DiscoveryReport {
        DiscoveryReport {
            schema_version: crate::RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            session_enumeration: crate::DiscoveryEnumerationStatus::Ok,
            process_observation: crate::DiscoveryEnumerationStatus::Ok,
            candidates: vec![DiscoveredRunner {
                local_discovery_key: "key-a".to_owned(),
                backend: RunnerBackendKind::UserSession,
                disposition: DiscoveryDisposition::ObservedOnly,
                refusal: None,
                identity: DiscoveredRunnerIdentity {
                    registration_scope: Some(RegistrationScope::Repository {
                        owner: "fixture-owner".to_owned(),
                        repository: "repo-a".to_owned(),
                    }),
                    runner_id: Some(7),
                    expected_runner_name: Some("fixture-7".to_owned()),
                    runner_home: PathBuf::from(r"C:\fixture\runners\a"),
                    work_root: Some(PathBuf::from(r"C:\fixture\runners\a\_work")),
                    execution_identity_ref: Some(identity()),
                },
                service: None,
                session: None,
                observed_lifecycle: DiscoveredLifecycle::Stopped,
                same_scope_collision: false,
                capabilities: DiscoveryCapabilities {
                    service_control_contract: false,
                    user_session_control_contract: true,
                    enrollment_eligible: true,
                },
            }],
        }
    }

    #[test]
    fn read_only_list_and_discover_do_not_mutate_registry() {
        let mut editor = RunnerRegistryEditor::empty();
        let discovery = discovery_report();
        let listed =
            handle_runners_command(&mut editor, Some(&discovery), &AgentCommand::RunnersList)
                .unwrap();
        assert!(listed.read_only);
        assert!(!listed.applied);
        assert!(listed.records.is_empty());

        let discovered = handle_runners_command(
            &mut editor,
            Some(&discovery),
            &AgentCommand::RunnersDiscover,
        )
        .unwrap();
        assert!(discovered.read_only);
        assert_eq!(discovered.discovered_keys, vec!["key-a".to_owned()]);
        assert_eq!(discovered.discovered_candidates.len(), 1);
        assert_eq!(
            discovered.discovered_candidates[0].disposition,
            "OBSERVED_ONLY"
        );
        assert!(discovered.discovered_candidates[0].enrollment_eligible);
        assert!(editor.registry().records.is_empty());
    }

    #[test]
    fn discover_surfaces_refused_candidates_without_enrollment() {
        let mut editor = RunnerRegistryEditor::empty();
        let mut discovery = discovery_report();
        discovery.candidates[0].disposition = DiscoveryDisposition::Refused;
        discovery.candidates[0].refusal = Some(crate::DiscoveryRefusalReason::DuplicateHome);
        discovery.candidates[0].capabilities.enrollment_eligible = false;
        let discovered = handle_runners_command(
            &mut editor,
            Some(&discovery),
            &AgentCommand::RunnersDiscover,
        )
        .unwrap();
        let refused = &discovered.discovered_candidates[0];
        assert_eq!(refused.disposition, "REFUSED");
        assert_eq!(
            refused.refusal_reason.as_deref(),
            Some("discovery.duplicate-home")
        );
        assert!(!refused.enrollment_eligible);
    }

    #[test]
    fn prepare_only_enroll_does_not_apply_but_apply_persists_intent() {
        let mut editor = RunnerRegistryEditor::empty();
        let discovery = discovery_report();
        let prepared = handle_runners_command(
            &mut editor,
            Some(&discovery),
            &AgentCommand::RunnersEnroll {
                local_runner_key: "key-a".to_owned(),
                prepare_only: true,
            },
        )
        .unwrap();
        assert!(prepared.prepare_only);
        assert!(!prepared.applied);
        assert!(editor.registry().records.is_empty());

        let applied = handle_runners_command(
            &mut editor,
            Some(&discovery),
            &AgentCommand::RunnersEnroll {
                local_runner_key: "key-a".to_owned(),
                prepare_only: false,
            },
        )
        .unwrap();
        assert!(applied.applied);
        assert_eq!(editor.registry().records.len(), 1);

        let disabled = handle_runners_command(
            &mut editor,
            None,
            &AgentCommand::RunnersDisable {
                local_runner_key: "key-a".to_owned(),
                when_idle: true,
                prepare_only: false,
            },
        )
        .unwrap();
        assert!(disabled.applied);
        assert!(!editor.registry().get("key-a").unwrap().desired_enabled);
    }
}
