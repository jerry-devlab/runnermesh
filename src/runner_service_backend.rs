//! Synthetic Windows Service lifecycle backend (LRM-04 source preparation).
//!
//! Real SCM mutation remains Owner-gated. This module only:
//! - models exact service identity/state contracts;
//! - plans narrow start/stop/startup changes behind prepare receipts;
//! - refuses wildcards, busy/unknown states, and foreign ownership.
//!
//! It never opens the Windows Service Control Manager.

use serde::{Deserialize, Serialize};

use crate::{DiscoveredLifecycle, RegistryBackendBinding, RunnerRecord};

/// Observed SCM start mode. Distinct from runtime phase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceStartMode {
    Automatic,
    Manual,
    Disabled,
    Unknown,
}

/// Exact service observation facts reconstructed for one enrolled runner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceObservation {
    pub service_name: String,
    pub state: DiscoveredLifecycle,
    pub start_mode: ServiceStartMode,
    pub image_path_matches: bool,
    pub ownership_verified: bool,
}

/// Narrow planned SCM action. Constructing a plan has no OS side effect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PreparedServiceAction {
    Start {
        service_name: String,
    },
    Stop {
        service_name: String,
    },
    SetStartMode {
        service_name: String,
        from: ServiceStartMode,
        to: ServiceStartMode,
    },
}

/// Why a service plan was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceBackendRefusal {
    NotWindowsServiceBackend,
    ServiceNameMissing,
    OwnershipUnverified,
    ImagePathMismatch,
    StateUnknown,
    BusyWorker,
    WildcardForbidden,
    AlreadyInDesiredState,
}

impl ServiceBackendRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotWindowsServiceBackend => "service.not-windows-service-backend",
            Self::ServiceNameMissing => "service.name-missing",
            Self::OwnershipUnverified => "service.ownership-unverified",
            Self::ImagePathMismatch => "service.image-path-mismatch",
            Self::StateUnknown => "service.state-unknown",
            Self::BusyWorker => "service.busy-worker",
            Self::WildcardForbidden => "service.wildcard-forbidden",
            Self::AlreadyInDesiredState => "service.already-in-desired-state",
        }
    }
}

/// Result of preparing a service operation without executing it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServicePrepareOutcome {
    Planned(PreparedServiceAction),
    Noop,
    Refused(ServiceBackendRefusal),
}

/// Read-only / prepare-only service backend. No SCM handle is retained.
#[derive(Clone, Debug, Default)]
pub struct SyntheticWindowsServiceBackend;

impl SyntheticWindowsServiceBackend {
    pub fn prepare_start(
        &self,
        record: &RunnerRecord,
        observed: &ServiceObservation,
    ) -> ServicePrepareOutcome {
        if let Some(refusal) = common_refusal(record, observed) {
            return ServicePrepareOutcome::Refused(refusal);
        }
        match observed.state {
            DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy => {
                ServicePrepareOutcome::Noop
            }
            DiscoveredLifecycle::Stopped => {
                ServicePrepareOutcome::Planned(PreparedServiceAction::Start {
                    service_name: observed.service_name.clone(),
                })
            }
            DiscoveredLifecycle::Unknown => {
                ServicePrepareOutcome::Refused(ServiceBackendRefusal::StateUnknown)
            }
        }
    }

    pub fn prepare_stop_when_idle(
        &self,
        record: &RunnerRecord,
        observed: &ServiceObservation,
    ) -> ServicePrepareOutcome {
        if let Some(refusal) = common_refusal(record, observed) {
            return ServicePrepareOutcome::Refused(refusal);
        }
        match observed.state {
            DiscoveredLifecycle::Stopped => ServicePrepareOutcome::Noop,
            DiscoveredLifecycle::Busy => {
                ServicePrepareOutcome::Refused(ServiceBackendRefusal::BusyWorker)
            }
            DiscoveredLifecycle::Unknown => {
                ServicePrepareOutcome::Refused(ServiceBackendRefusal::StateUnknown)
            }
            DiscoveredLifecycle::Listening => {
                ServicePrepareOutcome::Planned(PreparedServiceAction::Stop {
                    service_name: observed.service_name.clone(),
                })
            }
        }
    }

    pub fn prepare_set_start_mode(
        &self,
        record: &RunnerRecord,
        observed: &ServiceObservation,
        desired: ServiceStartMode,
    ) -> ServicePrepareOutcome {
        if let Some(refusal) = common_refusal(record, observed) {
            return ServicePrepareOutcome::Refused(refusal);
        }
        if matches!(desired, ServiceStartMode::Unknown)
            || matches!(observed.start_mode, ServiceStartMode::Unknown)
        {
            return ServicePrepareOutcome::Refused(ServiceBackendRefusal::StateUnknown);
        }
        if observed.start_mode == desired {
            return ServicePrepareOutcome::Noop;
        }
        ServicePrepareOutcome::Planned(PreparedServiceAction::SetStartMode {
            service_name: observed.service_name.clone(),
            from: observed.start_mode,
            to: desired,
        })
    }
}

fn common_refusal(
    record: &RunnerRecord,
    observed: &ServiceObservation,
) -> Option<ServiceBackendRefusal> {
    let RegistryBackendBinding::WindowsService { service_name } = &record.backend else {
        return Some(ServiceBackendRefusal::NotWindowsServiceBackend);
    };
    if service_name.trim().is_empty() || observed.service_name.trim().is_empty() {
        return Some(ServiceBackendRefusal::ServiceNameMissing);
    }
    if service_name.contains('*') || observed.service_name.contains('*') {
        return Some(ServiceBackendRefusal::WildcardForbidden);
    }
    if !service_name.eq_ignore_ascii_case(&observed.service_name) {
        return Some(ServiceBackendRefusal::ServiceNameMissing);
    }
    if !observed.ownership_verified {
        return Some(ServiceBackendRefusal::OwnershipUnverified);
    }
    if !observed.image_path_matches {
        return Some(ServiceBackendRefusal::ImagePathMismatch);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CredentialReference, EnrollmentState, OpaqueIdentityReference, RegistrationScope,
        WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
    };
    use std::path::PathBuf;

    fn record(service_name: &str) -> RunnerRecord {
        RunnerRecord {
            local_runner_key: "key-svc".to_owned(),
            registration_scope: RegistrationScope::Repository {
                owner: "fixture-owner".to_owned(),
                repository: "repo-a".to_owned(),
            },
            runner_id: 11,
            expected_runner_name: "fixture-svc".to_owned(),
            runner_home: PathBuf::from(r"C:\fixture\runners\svc"),
            work_root: PathBuf::from(r"C:\fixture\runners\svc\_work"),
            execution_identity_ref: OpaqueIdentityReference::new(
                WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .unwrap(),
            backend: RegistryBackendBinding::WindowsService {
                service_name: service_name.to_owned(),
            },
            desired_enabled: true,
            enrollment_state: EnrollmentState::Enrolled,
            credential_ref: Some(
                CredentialReference::new("windows-credential-manager", "fixture-reference")
                    .unwrap(),
            ),
            managed_admission: false,
        }
    }

    fn observation(state: DiscoveredLifecycle) -> ServiceObservation {
        ServiceObservation {
            service_name: "fixture.scm.generic".to_owned(),
            state,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
        }
    }

    #[test]
    fn prepare_stop_refuses_busy_and_unknown_without_scm_call() {
        let backend = SyntheticWindowsServiceBackend;
        let record = record("fixture.scm.generic");
        assert!(matches!(
            backend.prepare_stop_when_idle(&record, &observation(DiscoveredLifecycle::Busy)),
            ServicePrepareOutcome::Refused(ServiceBackendRefusal::BusyWorker)
        ));
        assert!(matches!(
            backend.prepare_stop_when_idle(&record, &observation(DiscoveredLifecycle::Unknown)),
            ServicePrepareOutcome::Refused(ServiceBackendRefusal::StateUnknown)
        ));
        assert_eq!(
            backend.prepare_stop_when_idle(&record, &observation(DiscoveredLifecycle::Listening)),
            ServicePrepareOutcome::Planned(PreparedServiceAction::Stop {
                service_name: "fixture.scm.generic".to_owned()
            })
        );
    }

    #[test]
    fn wildcard_and_unverified_ownership_are_refused() {
        let backend = SyntheticWindowsServiceBackend;
        let wild = record("actions.runner.*");
        assert!(matches!(
            backend.prepare_start(&wild, &observation(DiscoveredLifecycle::Stopped)),
            ServicePrepareOutcome::Refused(ServiceBackendRefusal::WildcardForbidden)
        ));
        let mut unverified = observation(DiscoveredLifecycle::Stopped);
        unverified.ownership_verified = false;
        assert!(matches!(
            backend.prepare_start(&record("fixture.scm.generic"), &unverified),
            ServicePrepareOutcome::Refused(ServiceBackendRefusal::OwnershipUnverified)
        ));
    }

    #[test]
    fn start_mode_change_is_transactional_plan_only() {
        let backend = SyntheticWindowsServiceBackend;
        let record = record("fixture.scm.generic");
        let observed = observation(DiscoveredLifecycle::Stopped);
        assert_eq!(
            backend.prepare_set_start_mode(&record, &observed, ServiceStartMode::Disabled),
            ServicePrepareOutcome::Planned(PreparedServiceAction::SetStartMode {
                service_name: "fixture.scm.generic".to_owned(),
                from: ServiceStartMode::Manual,
                to: ServiceStartMode::Disabled,
            })
        );
        assert_eq!(
            backend.prepare_set_start_mode(&record, &observed, ServiceStartMode::Manual),
            ServicePrepareOutcome::Noop
        );
    }
}
