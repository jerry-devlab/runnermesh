//! Read-only multi-instance local GitHub Actions runner discovery (LRM-01).
//!
//! This module inventories candidate Windows Service and user-session runners
//! behind injectable sources. It never starts, stops, configures, registers,
//! or otherwise mutates a runner, service, work root, or credential store.
//! Discovery is never claimed to be exhaustive. Enrollment and control belong
//! to later LRM goals; v0.1 single-binding contracts remain unchanged.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{OpaqueIdentityReference, RegistrationScope};

/// Schema marker for the pure discovery inventory contract.
pub const RUNNER_DISCOVERY_SCHEMA_VERSION: u32 = 1;

/// Backend kinds recognized by the local discovery contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunnerBackendKind {
    WindowsService,
    UserSession,
    /// Observed only; no control backend is defined yet.
    ScheduledTask,
    Unknown,
}

/// Lifecycle hint reconstructed from read-only observation facts.
///
/// `Unknown` is neither `Stopped` nor a safe-to-stop signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveredLifecycle {
    Stopped,
    Listening,
    Busy,
    Unknown,
}

/// Discovery disposition before any enrollment registry exists.
///
/// LRM-01 never emits `Enrolled`; that state requires LRM-02 persistence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryDisposition {
    ObservedOnly,
    Refused,
}

/// Stable refusal reasons for discovery candidates. Values are non-localized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryRefusalReason {
    DuplicateHome,
    DuplicateServiceName,
    DuplicateServerIdentity,
    ForeignExecutionIdentity,
    IdentityEvidenceMissing,
    AmbiguousBinding,
    UnsupportedBackend,
}

impl DiscoveryRefusalReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateHome => "discovery.duplicate-home",
            Self::DuplicateServiceName => "discovery.duplicate-service-name",
            Self::DuplicateServerIdentity => "discovery.duplicate-server-identity",
            Self::ForeignExecutionIdentity => "discovery.foreign-execution-identity",
            Self::IdentityEvidenceMissing => "discovery.identity-evidence-missing",
            Self::AmbiguousBinding => "discovery.ambiguous-binding",
            Self::UnsupportedBackend => "discovery.unsupported-backend",
        }
    }
}

impl fmt::Display for DiscoveryRefusalReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Capability flags describing what later Goals may attempt. Discovery itself
/// never exercises control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryCapabilities {
    pub service_control_contract: bool,
    pub user_session_control_contract: bool,
    pub enrollment_eligible: bool,
}

impl DiscoveryCapabilities {
    fn for_backend(backend: RunnerBackendKind, enrollment_eligible: bool) -> Self {
        match backend {
            RunnerBackendKind::WindowsService => Self {
                service_control_contract: true,
                user_session_control_contract: false,
                enrollment_eligible,
            },
            RunnerBackendKind::UserSession => Self {
                service_control_contract: false,
                user_session_control_contract: true,
                enrollment_eligible,
            },
            RunnerBackendKind::ScheduledTask | RunnerBackendKind::Unknown => Self {
                service_control_contract: false,
                user_session_control_contract: false,
                enrollment_eligible: false,
            },
        }
    }
}

/// Exact Windows Service binding facts. Names only; no SCM handle is retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredServiceBinding {
    pub service_name: String,
    pub display_name: Option<String>,
    pub image_path: Option<PathBuf>,
    pub state: DiscoveredLifecycle,
}

/// Owned user-session binding facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredSessionBinding {
    pub current_user_owned: bool,
    pub listener_present: bool,
    pub worker_present: bool,
}

/// Non-secret identity evidence reconstructed for a candidate.
///
/// Omits `Debug` because nested opaque identity references are intentionally
/// non-Debug.
#[derive(Clone, Eq, PartialEq)]
pub struct DiscoveredRunnerIdentity {
    pub registration_scope: Option<RegistrationScope>,
    pub runner_id: Option<u64>,
    pub expected_runner_name: Option<String>,
    pub runner_home: PathBuf,
    pub work_root: Option<PathBuf>,
    pub execution_identity_ref: Option<OpaqueIdentityReference>,
}

/// One discovered local runner candidate.
#[derive(Clone, Eq, PartialEq)]
pub struct DiscoveredRunner {
    /// Deterministic local key derived from backend + home (+ service name).
    /// Never a process ID.
    pub local_discovery_key: String,
    pub backend: RunnerBackendKind,
    pub disposition: DiscoveryDisposition,
    pub refusal: Option<DiscoveryRefusalReason>,
    pub identity: DiscoveredRunnerIdentity,
    pub service: Option<DiscoveredServiceBinding>,
    pub session: Option<DiscoveredSessionBinding>,
    pub observed_lifecycle: DiscoveredLifecycle,
    /// True when another candidate shares the same non-empty registration scope.
    /// Discovery remains safe; managed-admission enrollment is later blocked.
    pub same_scope_collision: bool,
    pub capabilities: DiscoveryCapabilities,
}

/// Host-side enumeration health. Empty candidate lists are only trustworthy
/// when every related status is `Ok`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiscoveryEnumerationStatus {
    /// Enumeration completed; zero candidates may be a true empty inventory.
    #[default]
    Ok,
    /// Some evidence was collected, but gaps remain (skipped services, missing
    /// process snapshot, partial identity).
    Incomplete,
    /// The host API could not be queried. Must never be presented as success.
    Unavailable,
}

/// Inventory result. `exhaustive` is always false by contract.
#[derive(Clone, Eq, PartialEq)]
pub struct DiscoveryReport {
    pub schema_version: u32,
    pub read_only: bool,
    pub exhaustive: bool,
    pub service_enumeration: DiscoveryEnumerationStatus,
    pub session_enumeration: DiscoveryEnumerationStatus,
    pub process_observation: DiscoveryEnumerationStatus,
    pub candidates: Vec<DiscoveredRunner>,
}

impl DiscoveryReport {
    pub fn empty_ok() -> Self {
        Self {
            schema_version: RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: DiscoveryEnumerationStatus::Ok,
            session_enumeration: DiscoveryEnumerationStatus::Ok,
            process_observation: DiscoveryEnumerationStatus::Ok,
            candidates: Vec::new(),
        }
    }

    /// True when any enumeration channel failed or was incomplete.
    pub fn is_incomplete(&self) -> bool {
        !matches!(self.service_enumeration, DiscoveryEnumerationStatus::Ok)
            || !matches!(self.session_enumeration, DiscoveryEnumerationStatus::Ok)
            || !matches!(self.process_observation, DiscoveryEnumerationStatus::Ok)
    }

    /// False empty success: unavailable service enumeration with zero candidates.
    pub fn is_false_empty_success(&self) -> bool {
        self.candidates.is_empty()
            && matches!(
                self.service_enumeration,
                DiscoveryEnumerationStatus::Unavailable
            )
    }

    pub fn observed_only(&self) -> impl Iterator<Item = &DiscoveredRunner> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.disposition == DiscoveryDisposition::ObservedOnly)
    }

    pub fn refused(&self) -> impl Iterator<Item = &DiscoveredRunner> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.disposition == DiscoveryDisposition::Refused)
    }

    pub fn identity_complete_count(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| exact_identity_present(&candidate.identity))
            .count()
    }
}

/// Raw Windows Service candidate supplied by an injectable source.
#[derive(Clone, Eq, PartialEq)]
pub struct ServiceCandidateFact {
    pub service_name: String,
    pub display_name: Option<String>,
    pub image_path: Option<PathBuf>,
    pub runner_home: PathBuf,
    pub work_root: Option<PathBuf>,
    pub registration_scope: Option<RegistrationScope>,
    pub runner_id: Option<u64>,
    pub expected_runner_name: Option<String>,
    pub execution_identity_ref: Option<OpaqueIdentityReference>,
    pub state: DiscoveredLifecycle,
}

/// Raw user-session candidate supplied by an injectable source.
#[derive(Clone, Eq, PartialEq)]
pub struct SessionCandidateFact {
    pub runner_home: PathBuf,
    pub work_root: Option<PathBuf>,
    pub registration_scope: Option<RegistrationScope>,
    pub runner_id: Option<u64>,
    pub expected_runner_name: Option<String>,
    pub execution_identity_ref: Option<OpaqueIdentityReference>,
    pub current_user_owned: bool,
    pub listener_present: bool,
    pub worker_present: bool,
    pub state: DiscoveredLifecycle,
}

/// Raw scheduled-task / unsupported candidate. Always observation-only or refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedCandidateFact {
    pub runner_home: PathBuf,
    pub work_root: Option<PathBuf>,
    pub registration_scope: Option<RegistrationScope>,
    pub runner_id: Option<u64>,
    pub expected_runner_name: Option<String>,
    pub label: String,
    pub state: DiscoveredLifecycle,
}

/// Read-only Windows Service discovery source. Implementations must not expose
/// start/stop/configure operations.
pub trait ServiceDiscoverySource {
    fn list_service_candidates(&self) -> Vec<ServiceCandidateFact>;
    /// Host enumeration health. Defaults to `Ok` for synthetic fixtures.
    fn enumeration_status(&self) -> DiscoveryEnumerationStatus {
        DiscoveryEnumerationStatus::Ok
    }
}

/// Read-only user-session discovery source.
pub trait SessionDiscoverySource {
    fn list_session_candidates(&self) -> Vec<SessionCandidateFact>;
    fn enumeration_status(&self) -> DiscoveryEnumerationStatus {
        DiscoveryEnumerationStatus::Ok
    }
}

/// Read-only unsupported-mode discovery source (scheduled tasks, etc.).
pub trait UnsupportedDiscoverySource {
    fn list_unsupported_candidates(&self) -> Vec<UnsupportedCandidateFact>;
}

/// Empty sources used when a backend is not configured for a test or adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyServiceDiscoverySource;

impl ServiceDiscoverySource for EmptyServiceDiscoverySource {
    fn list_service_candidates(&self) -> Vec<ServiceCandidateFact> {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EmptySessionDiscoverySource;

impl SessionDiscoverySource for EmptySessionDiscoverySource {
    fn list_session_candidates(&self) -> Vec<SessionCandidateFact> {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyUnsupportedDiscoverySource;

impl UnsupportedDiscoverySource for EmptyUnsupportedDiscoverySource {
    fn list_unsupported_candidates(&self) -> Vec<UnsupportedCandidateFact> {
        Vec::new()
    }
}

/// Injectable synthetic Service discovery backend for deterministic fixtures.
#[derive(Clone, Default)]
pub struct SyntheticServiceDiscoverySource {
    candidates: Vec<ServiceCandidateFact>,
}

impl SyntheticServiceDiscoverySource {
    pub fn new(candidates: Vec<ServiceCandidateFact>) -> Self {
        Self { candidates }
    }
}

impl ServiceDiscoverySource for SyntheticServiceDiscoverySource {
    fn list_service_candidates(&self) -> Vec<ServiceCandidateFact> {
        self.candidates.clone()
    }
}

/// Injectable synthetic user-session discovery backend.
#[derive(Clone, Default)]
pub struct SyntheticSessionDiscoverySource {
    candidates: Vec<SessionCandidateFact>,
}

impl SyntheticSessionDiscoverySource {
    pub fn new(candidates: Vec<SessionCandidateFact>) -> Self {
        Self { candidates }
    }
}

impl SessionDiscoverySource for SyntheticSessionDiscoverySource {
    fn list_session_candidates(&self) -> Vec<SessionCandidateFact> {
        self.candidates.clone()
    }
}

/// Injectable synthetic unsupported-mode discovery backend.
#[derive(Clone, Debug, Default)]
pub struct SyntheticUnsupportedDiscoverySource {
    candidates: Vec<UnsupportedCandidateFact>,
}

impl SyntheticUnsupportedDiscoverySource {
    pub fn new(candidates: Vec<UnsupportedCandidateFact>) -> Self {
        Self { candidates }
    }
}

impl UnsupportedDiscoverySource for SyntheticUnsupportedDiscoverySource {
    fn list_unsupported_candidates(&self) -> Vec<UnsupportedCandidateFact> {
        self.candidates.clone()
    }
}

/// Optional expected execution identity used to refuse foreign-session runners.
///
/// Deliberately omits `Debug` because `OpaqueIdentityReference` is non-Debug
/// to avoid leaking identity material into logs.
#[derive(Clone, Default)]
pub struct DiscoveryIdentityFilter {
    pub expected_execution_identity: Option<OpaqueIdentityReference>,
}

/// Pure read-only inventory over injectable discovery sources.
#[derive(Clone)]
pub struct RunnerDiscoveryInventory<Svc, Sess, Unsup> {
    service_source: Svc,
    session_source: Sess,
    unsupported_source: Unsup,
    identity_filter: DiscoveryIdentityFilter,
}

impl<Svc, Sess, Unsup> RunnerDiscoveryInventory<Svc, Sess, Unsup> {
    pub fn new(
        service_source: Svc,
        session_source: Sess,
        unsupported_source: Unsup,
        identity_filter: DiscoveryIdentityFilter,
    ) -> Self {
        Self {
            service_source,
            session_source,
            unsupported_source,
            identity_filter,
        }
    }
}

impl<Svc, Sess, Unsup> RunnerDiscoveryInventory<Svc, Sess, Unsup>
where
    Svc: ServiceDiscoverySource,
    Sess: SessionDiscoverySource,
    Unsup: UnsupportedDiscoverySource,
{
    /// Collects, normalizes, and classifies candidates. Side-effect free with
    /// respect to the host: sources are only read through list APIs.
    pub fn discover(&self) -> DiscoveryReport {
        let mut drafts = Vec::new();
        for fact in self.service_source.list_service_candidates() {
            drafts.push(draft_from_service(fact));
        }
        for fact in self.session_source.list_session_candidates() {
            drafts.push(draft_from_session(fact));
        }
        for fact in self.unsupported_source.list_unsupported_candidates() {
            drafts.push(draft_from_unsupported(fact));
        }

        apply_identity_filter(&mut drafts, &self.identity_filter);
        apply_collision_refusals(&mut drafts);
        mark_same_scope_collisions(&mut drafts);

        drafts.sort_by(|left, right| {
            left.local_discovery_key
                .cmp(&right.local_discovery_key)
                .then_with(|| left.identity.runner_home.cmp(&right.identity.runner_home))
        });

        debug_assert!(read_only_boundary::discovery_is_read_only::<Self>());
        DiscoveryReport {
            schema_version: RUNNER_DISCOVERY_SCHEMA_VERSION,
            read_only: true,
            exhaustive: false,
            service_enumeration: self.service_source.enumeration_status(),
            session_enumeration: self.session_source.enumeration_status(),
            process_observation: DiscoveryEnumerationStatus::Ok,
            candidates: drafts,
        }
    }

    /// Like [`discover`](Self::discover), but allows the Windows adapter to
    /// override process-observation health after a live process snapshot.
    pub fn discover_with_process_status(
        &self,
        process_observation: DiscoveryEnumerationStatus,
    ) -> DiscoveryReport {
        let mut report = self.discover();
        report.process_observation = process_observation;
        report
    }
}

fn draft_from_service(fact: ServiceCandidateFact) -> DiscoveredRunner {
    let backend = RunnerBackendKind::WindowsService;
    let mut refusal = None;
    if fact.service_name.trim().is_empty() || !path_usable(&fact.runner_home) {
        refusal = Some(DiscoveryRefusalReason::IdentityEvidenceMissing);
    }
    let disposition = if refusal.is_some() {
        DiscoveryDisposition::Refused
    } else {
        DiscoveryDisposition::ObservedOnly
    };
    let identity = DiscoveredRunnerIdentity {
        registration_scope: fact.registration_scope,
        runner_id: fact.runner_id,
        expected_runner_name: fact.expected_runner_name,
        runner_home: fact.runner_home,
        work_root: fact.work_root,
        execution_identity_ref: fact.execution_identity_ref,
    };
    let enrollment_eligible = enrollment_eligible_for(disposition, backend, &identity);
    let key = discovery_key(
        backend,
        &identity.runner_home,
        Some(fact.service_name.as_str()),
        None,
    );
    DiscoveredRunner {
        local_discovery_key: key,
        backend,
        disposition,
        refusal,
        identity,
        service: Some(DiscoveredServiceBinding {
            service_name: fact.service_name,
            display_name: fact.display_name,
            image_path: fact.image_path,
            state: fact.state,
        }),
        session: None,
        observed_lifecycle: fact.state,
        same_scope_collision: false,
        capabilities: DiscoveryCapabilities::for_backend(backend, enrollment_eligible),
    }
}

fn draft_from_session(fact: SessionCandidateFact) -> DiscoveredRunner {
    let backend = RunnerBackendKind::UserSession;
    let mut refusal = None;
    if !path_usable(&fact.runner_home) {
        refusal = Some(DiscoveryRefusalReason::IdentityEvidenceMissing);
    } else if !fact.current_user_owned {
        refusal = Some(DiscoveryRefusalReason::ForeignExecutionIdentity);
    }
    let disposition = if refusal.is_some() {
        DiscoveryDisposition::Refused
    } else {
        DiscoveryDisposition::ObservedOnly
    };
    // Positive process evidence may refine Unknown into Busy/Listening, but
    // absence must not invent Stopped from Unknown.
    let lifecycle = match fact.state {
        DiscoveredLifecycle::Unknown if fact.worker_present => DiscoveredLifecycle::Busy,
        DiscoveredLifecycle::Unknown if fact.listener_present => DiscoveredLifecycle::Listening,
        other => other,
    };
    let identity = DiscoveredRunnerIdentity {
        registration_scope: fact.registration_scope,
        runner_id: fact.runner_id,
        expected_runner_name: fact.expected_runner_name,
        runner_home: fact.runner_home,
        work_root: fact.work_root,
        execution_identity_ref: fact.execution_identity_ref,
    };
    let enrollment_eligible = enrollment_eligible_for(disposition, backend, &identity);
    let key = discovery_key(backend, &identity.runner_home, None, None);
    DiscoveredRunner {
        local_discovery_key: key,
        backend,
        disposition,
        refusal,
        identity,
        service: None,
        session: Some(DiscoveredSessionBinding {
            current_user_owned: fact.current_user_owned,
            listener_present: fact.listener_present,
            worker_present: fact.worker_present,
        }),
        observed_lifecycle: lifecycle,
        same_scope_collision: false,
        capabilities: DiscoveryCapabilities::for_backend(backend, enrollment_eligible),
    }
}

fn draft_from_unsupported(fact: UnsupportedCandidateFact) -> DiscoveredRunner {
    let backend = RunnerBackendKind::ScheduledTask;
    // Scheduled-task / unsupported modes stay visible as ObservedOnly when the
    // home is usable, but are never enrollment-eligible. Missing path evidence
    // fails closed as Refused.
    let (disposition, refusal) = if path_usable(&fact.runner_home) {
        (DiscoveryDisposition::ObservedOnly, None)
    } else {
        (
            DiscoveryDisposition::Refused,
            Some(DiscoveryRefusalReason::IdentityEvidenceMissing),
        )
    };
    let key = discovery_key(backend, &fact.runner_home, None, Some(fact.label.as_str()));
    DiscoveredRunner {
        local_discovery_key: key,
        backend,
        disposition,
        refusal,
        identity: DiscoveredRunnerIdentity {
            registration_scope: fact.registration_scope,
            runner_id: fact.runner_id,
            expected_runner_name: fact.expected_runner_name,
            runner_home: fact.runner_home,
            work_root: fact.work_root,
            execution_identity_ref: None,
        },
        service: None,
        session: None,
        observed_lifecycle: fact.state,
        same_scope_collision: false,
        capabilities: DiscoveryCapabilities::for_backend(backend, false),
    }
}

fn apply_identity_filter(drafts: &mut [DiscoveredRunner], filter: &DiscoveryIdentityFilter) {
    let Some(expected) = filter.expected_execution_identity.as_ref() else {
        return;
    };
    for draft in drafts.iter_mut() {
        if draft.disposition == DiscoveryDisposition::Refused {
            continue;
        }
        // Observation-only unsupported backends never enroll; missing identity
        // there is not an additional refusal beyond enrollment_eligible=false.
        if matches!(
            draft.backend,
            RunnerBackendKind::ScheduledTask | RunnerBackendKind::Unknown
        ) {
            continue;
        }
        match draft.identity.execution_identity_ref.as_ref() {
            Some(observed) if observed == expected => {}
            Some(_) => refuse(draft, DiscoveryRefusalReason::ForeignExecutionIdentity),
            None => refuse(draft, DiscoveryRefusalReason::IdentityEvidenceMissing),
        }
    }
}

fn apply_collision_refusals(drafts: &mut [DiscoveredRunner]) {
    let mut home_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut service_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut server_counts: BTreeMap<String, usize> = BTreeMap::new();

    for draft in drafts.iter() {
        *home_counts
            .entry(normalize_path_key(&draft.identity.runner_home))
            .or_insert(0) += 1;
        if let Some(service) = draft.service.as_ref() {
            *service_counts
                .entry(service.service_name.to_ascii_lowercase())
                .or_insert(0) += 1;
        }
        if let Some(key) = server_identity_key(&draft.identity) {
            *server_counts.entry(key).or_insert(0) += 1;
        }
    }

    for draft in drafts.iter_mut() {
        if draft.disposition == DiscoveryDisposition::Refused {
            continue;
        }
        let home_key = normalize_path_key(&draft.identity.runner_home);
        if home_counts.get(&home_key).copied().unwrap_or(0) > 1 {
            refuse(draft, DiscoveryRefusalReason::DuplicateHome);
            continue;
        }
        if let Some(service) = draft.service.as_ref() {
            let service_key = service.service_name.to_ascii_lowercase();
            if service_counts.get(&service_key).copied().unwrap_or(0) > 1 {
                refuse(draft, DiscoveryRefusalReason::DuplicateServiceName);
                continue;
            }
        }
        if let Some(key) = server_identity_key(&draft.identity) {
            if server_counts.get(&key).copied().unwrap_or(0) > 1 {
                refuse(draft, DiscoveryRefusalReason::DuplicateServerIdentity);
            }
        }
    }
}

fn mark_same_scope_collisions(drafts: &mut [DiscoveredRunner]) {
    let mut scope_counts: BTreeMap<String, usize> = BTreeMap::new();
    for draft in drafts.iter() {
        if let Some(scope_key) = scope_key(draft.identity.registration_scope.as_ref()) {
            *scope_counts.entry(scope_key).or_insert(0) += 1;
        }
    }
    for draft in drafts.iter_mut() {
        if let Some(scope_key) = scope_key(draft.identity.registration_scope.as_ref()) {
            draft.same_scope_collision = scope_counts.get(&scope_key).copied().unwrap_or(0) > 1;
        }
    }
}

fn refuse(runner: &mut DiscoveredRunner, reason: DiscoveryRefusalReason) {
    runner.disposition = DiscoveryDisposition::Refused;
    runner.refusal = Some(reason);
    runner.capabilities.enrollment_eligible = false;
}

fn server_identity_key(identity: &DiscoveredRunnerIdentity) -> Option<String> {
    let scope = scope_key(identity.registration_scope.as_ref())?;
    let runner_id = identity.runner_id?;
    Some(format!("{scope}#{runner_id}"))
}

fn scope_key(scope: Option<&RegistrationScope>) -> Option<String> {
    match scope? {
        RegistrationScope::Organization { organization } => {
            Some(format!("org:{}", organization.to_ascii_lowercase()))
        }
        RegistrationScope::Repository { owner, repository } => Some(format!(
            "repo:{}/{}",
            owner.to_ascii_lowercase(),
            repository.to_ascii_lowercase()
        )),
    }
}

fn enrollment_eligible_for(
    disposition: DiscoveryDisposition,
    backend: RunnerBackendKind,
    identity: &DiscoveredRunnerIdentity,
) -> bool {
    if disposition != DiscoveryDisposition::ObservedOnly {
        return false;
    }
    if !matches!(
        backend,
        RunnerBackendKind::WindowsService | RunnerBackendKind::UserSession
    ) {
        return false;
    }
    exact_identity_present(identity)
}

fn exact_identity_present(identity: &DiscoveredRunnerIdentity) -> bool {
    identity.registration_scope.is_some()
        && identity.runner_id.is_some()
        && identity.execution_identity_ref.is_some()
        && path_usable(&identity.runner_home)
}

fn discovery_key(
    backend: RunnerBackendKind,
    runner_home: &Path,
    service_name: Option<&str>,
    unsupported_label: Option<&str>,
) -> String {
    let backend_token = match backend {
        RunnerBackendKind::WindowsService => "windows-service",
        RunnerBackendKind::UserSession => "user-session",
        RunnerBackendKind::ScheduledTask => "scheduled-task",
        RunnerBackendKind::Unknown => "unknown",
    };
    // Hash path material so local filesystem paths never appear in keys that
    // may later surface in public receipts or shared evidence.
    let home_token = opaque_path_token(runner_home);
    match (service_name, unsupported_label) {
        (Some(service), _) => format!(
            "{backend_token}|home:{home_token}|svc:{}",
            opaque_token(service)
        ),
        (_, Some(label)) => format!(
            "{backend_token}|home:{home_token}|label:{}",
            opaque_token(label)
        ),
        _ => format!("{backend_token}|home:{home_token}"),
    }
}

fn opaque_path_token(path: &Path) -> String {
    opaque_token(&normalize_path_key(path))
}

fn opaque_token(value: &str) -> String {
    let digest = Sha256::digest(value.to_ascii_lowercase().as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn path_usable(path: &Path) -> bool {
    let raw = path.as_os_str();
    !raw.is_empty() && !path.to_string_lossy().contains('\0')
}

/// Compile-time reminder that discovery sources expose no mutation surface.
/// Tests assert the synthetic backends only implement list APIs.
pub mod read_only_boundary {
    /// Marker trait implemented only by discovery inventory types.
    pub trait ReadOnlyDiscovery {
        const READ_ONLY: bool = true;
        const MAY_MUTATE_SERVICE: bool = false;
        const MAY_MUTATE_REGISTRATION: bool = false;
        const MAY_READ_CREDENTIALS: bool = false;
    }

    impl<Svc, Sess, Unsup> ReadOnlyDiscovery for super::RunnerDiscoveryInventory<Svc, Sess, Unsup> {}

    pub(super) fn discovery_is_read_only<T: ReadOnlyDiscovery>() -> bool {
        T::READ_ONLY
            && !T::MAY_MUTATE_SERVICE
            && !T::MAY_MUTATE_REGISTRATION
            && !T::MAY_READ_CREDENTIALS
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER;

    fn owned_identity() -> OpaqueIdentityReference {
        OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap()
    }

    fn foreign_identity() -> OpaqueIdentityReference {
        OpaqueIdentityReference::new(
            WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .unwrap()
    }

    fn repo_scope(owner: &str, repository: &str) -> RegistrationScope {
        RegistrationScope::Repository {
            owner: owner.to_owned(),
            repository: repository.to_owned(),
        }
    }

    fn service_fact(
        name: &str,
        home: &str,
        scope: RegistrationScope,
        runner_id: u64,
        state: DiscoveredLifecycle,
    ) -> ServiceCandidateFact {
        ServiceCandidateFact {
            service_name: name.to_owned(),
            display_name: Some(format!("fixture {name}")),
            image_path: Some(PathBuf::from(home).join("bin").join("Runner.Listener.exe")),
            runner_home: PathBuf::from(home),
            work_root: Some(PathBuf::from(home).join("_work")),
            registration_scope: Some(scope),
            runner_id: Some(runner_id),
            expected_runner_name: Some(format!("fixture-{name}")),
            execution_identity_ref: Some(owned_identity()),
            state,
        }
    }

    fn session_fact(
        home: &str,
        scope: RegistrationScope,
        runner_id: u64,
        current_user_owned: bool,
        state: DiscoveredLifecycle,
    ) -> SessionCandidateFact {
        SessionCandidateFact {
            runner_home: PathBuf::from(home),
            work_root: Some(PathBuf::from(home).join("_work")),
            registration_scope: Some(scope),
            runner_id: Some(runner_id),
            expected_runner_name: Some("fixture-session".to_owned()),
            execution_identity_ref: Some(if current_user_owned {
                owned_identity()
            } else {
                foreign_identity()
            }),
            current_user_owned,
            listener_present: matches!(
                state,
                DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
            ),
            worker_present: state == DiscoveredLifecycle::Busy,
            state,
        }
    }

    fn inventory(
        services: Vec<ServiceCandidateFact>,
        sessions: Vec<SessionCandidateFact>,
        unsupported: Vec<UnsupportedCandidateFact>,
    ) -> RunnerDiscoveryInventory<
        SyntheticServiceDiscoverySource,
        SyntheticSessionDiscoverySource,
        SyntheticUnsupportedDiscoverySource,
    > {
        RunnerDiscoveryInventory::new(
            SyntheticServiceDiscoverySource::new(services),
            SyntheticSessionDiscoverySource::new(sessions),
            SyntheticUnsupportedDiscoverySource::new(unsupported),
            DiscoveryIdentityFilter {
                expected_execution_identity: Some(owned_identity()),
            },
        )
    }

    #[test]
    fn zero_candidates_yields_empty_non_exhaustive_report() {
        let report = inventory(vec![], vec![], vec![]).discover();
        assert!(report.read_only);
        assert!(!report.exhaustive);
        assert_eq!(report.schema_version, RUNNER_DISCOVERY_SCHEMA_VERSION);
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn one_service_candidate_is_observed_only() {
        let report = inventory(
            vec![service_fact(
                "fixture.scm.generic-a",
                r"C:\fixture\runners\a",
                repo_scope("fixture-owner", "repo-a"),
                101,
                DiscoveredLifecycle::Listening,
            )],
            vec![],
            vec![],
        )
        .discover();
        assert_eq!(report.candidates.len(), 1);
        let candidate = &report.candidates[0];
        assert_eq!(candidate.disposition, DiscoveryDisposition::ObservedOnly);
        assert!(candidate.refusal.is_none());
        assert_eq!(candidate.backend, RunnerBackendKind::WindowsService);
        assert!(candidate.capabilities.service_control_contract);
        assert!(!candidate.capabilities.user_session_control_contract);
        assert!(candidate.capabilities.enrollment_eligible);
        assert!(!candidate.same_scope_collision);
        assert_eq!(candidate.observed_lifecycle, DiscoveredLifecycle::Listening);
    }

    #[test]
    fn n_instances_across_service_and_session_backends() {
        let report = inventory(
            vec![
                service_fact(
                    "fixture.scm.generic-a",
                    r"C:\fixture\runners\a",
                    repo_scope("fixture-owner", "repo-a"),
                    101,
                    DiscoveredLifecycle::Stopped,
                ),
                service_fact(
                    "fixture.scm.generic-b",
                    r"C:\fixture\runners\b",
                    repo_scope("fixture-owner", "repo-b"),
                    102,
                    DiscoveredLifecycle::Busy,
                ),
            ],
            vec![session_fact(
                r"C:\fixture\runners\c",
                repo_scope("fixture-owner", "repo-c"),
                103,
                true,
                DiscoveredLifecycle::Listening,
            )],
            vec![],
        )
        .discover();
        assert_eq!(report.candidates.len(), 3);
        assert_eq!(report.observed_only().count(), 3);
        assert!(report.candidates.iter().any(|c| {
            c.backend == RunnerBackendKind::UserSession
                && c.capabilities.user_session_control_contract
        }));
    }

    #[test]
    fn duplicate_home_is_refused() {
        let report = inventory(
            vec![service_fact(
                "fixture.scm.generic-a",
                r"C:\fixture\runners\shared",
                repo_scope("fixture-owner", "repo-a"),
                101,
                DiscoveredLifecycle::Listening,
            )],
            vec![session_fact(
                r"C:\fixture\runners\shared",
                repo_scope("fixture-owner", "repo-b"),
                102,
                true,
                DiscoveredLifecycle::Stopped,
            )],
            vec![],
        )
        .discover();
        assert_eq!(report.candidates.len(), 2);
        assert!(report.candidates.iter().all(|c| {
            c.disposition == DiscoveryDisposition::Refused
                && c.refusal == Some(DiscoveryRefusalReason::DuplicateHome)
                && !c.capabilities.enrollment_eligible
        }));
    }

    #[test]
    fn duplicate_service_name_is_refused() {
        let mut first = service_fact(
            "actions.runner.duplicate",
            r"C:\fixture\runners\a",
            repo_scope("fixture-owner", "repo-a"),
            101,
            DiscoveredLifecycle::Listening,
        );
        let mut second = service_fact(
            "ACTIONS.RUNNER.DUPLICATE",
            r"C:\fixture\runners\b",
            repo_scope("fixture-owner", "repo-b"),
            102,
            DiscoveredLifecycle::Stopped,
        );
        // Keep distinct homes/ids so only the service-name collision fires.
        first.runner_id = Some(201);
        second.runner_id = Some(202);
        let report = inventory(vec![first, second], vec![], vec![]).discover();
        assert!(report.candidates.iter().all(|c| {
            c.refusal == Some(DiscoveryRefusalReason::DuplicateServiceName)
                && c.disposition == DiscoveryDisposition::Refused
        }));
    }

    #[test]
    fn duplicate_server_identity_is_refused() {
        let scope = repo_scope("fixture-owner", "same-repo");
        let report = inventory(
            vec![service_fact(
                "fixture.scm.generic-a",
                r"C:\fixture\runners\a",
                scope.clone(),
                42,
                DiscoveredLifecycle::Listening,
            )],
            vec![session_fact(
                r"C:\fixture\runners\b",
                scope,
                42,
                true,
                DiscoveredLifecycle::Stopped,
            )],
            vec![],
        )
        .discover();
        assert!(report
            .candidates
            .iter()
            .all(|c| { c.refusal == Some(DiscoveryRefusalReason::DuplicateServerIdentity) }));
    }

    #[test]
    fn same_scope_distinct_ids_remain_observed_with_collision_flag() {
        let scope = repo_scope("fixture-owner", "shared-scope");
        let report = inventory(
            vec![
                service_fact(
                    "fixture.scm.generic-a",
                    r"C:\fixture\runners\a",
                    scope.clone(),
                    101,
                    DiscoveredLifecycle::Listening,
                ),
                service_fact(
                    "fixture.scm.generic-b",
                    r"C:\fixture\runners\b",
                    scope,
                    102,
                    DiscoveredLifecycle::Stopped,
                ),
            ],
            vec![],
            vec![],
        )
        .discover();
        assert_eq!(report.observed_only().count(), 2);
        assert!(report
            .candidates
            .iter()
            .all(|candidate| candidate.same_scope_collision));
    }

    #[test]
    fn unknown_lifecycle_is_preserved_and_not_treated_as_stopped() {
        let report = inventory(
            vec![service_fact(
                "fixture.scm.generic-unknown",
                r"C:\fixture\runners\unknown",
                repo_scope("fixture-owner", "repo-unknown"),
                777,
                DiscoveredLifecycle::Unknown,
            )],
            vec![],
            vec![],
        )
        .discover();
        let candidate = &report.candidates[0];
        assert_eq!(candidate.observed_lifecycle, DiscoveredLifecycle::Unknown);
        assert_ne!(candidate.observed_lifecycle, DiscoveredLifecycle::Stopped);
        assert_eq!(candidate.disposition, DiscoveryDisposition::ObservedOnly);
    }

    #[test]
    fn foreign_session_process_is_refused() {
        let report = inventory(
            vec![],
            vec![session_fact(
                r"C:\fixture\runners\foreign",
                repo_scope("fixture-owner", "repo-foreign"),
                55,
                false,
                DiscoveredLifecycle::Listening,
            )],
            vec![],
        )
        .discover();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(
            report.candidates[0].refusal,
            Some(DiscoveryRefusalReason::ForeignExecutionIdentity)
        );
        assert_eq!(
            report.candidates[0].disposition,
            DiscoveryDisposition::Refused
        );
        assert!(!report.candidates[0].capabilities.enrollment_eligible);
    }

    #[test]
    fn identity_filter_refuses_mismatched_execution_identity() {
        let mut fact = service_fact(
            "fixture.scm.generic-mismatch",
            r"C:\fixture\runners\mismatch",
            repo_scope("fixture-owner", "repo-mismatch"),
            88,
            DiscoveredLifecycle::Listening,
        );
        fact.execution_identity_ref = Some(foreign_identity());
        let report = inventory(vec![fact], vec![], vec![]).discover();
        assert_eq!(
            report.candidates[0].refusal,
            Some(DiscoveryRefusalReason::ForeignExecutionIdentity)
        );
    }

    #[test]
    fn unsupported_scheduled_task_is_observation_only_not_enrollment_eligible() {
        let report = inventory(
            vec![],
            vec![],
            vec![UnsupportedCandidateFact {
                runner_home: PathBuf::from(r"C:\fixture\runners\task"),
                work_root: Some(PathBuf::from(r"C:\fixture\runners\task\_work")),
                registration_scope: Some(repo_scope("fixture-owner", "repo-task")),
                runner_id: Some(9),
                expected_runner_name: Some("fixture-task".to_owned()),
                label: "fixture-task".to_owned(),
                state: DiscoveredLifecycle::Unknown,
            }],
        )
        .discover();
        assert_eq!(report.candidates.len(), 1);
        let candidate = &report.candidates[0];
        assert_eq!(candidate.backend, RunnerBackendKind::ScheduledTask);
        assert_eq!(candidate.disposition, DiscoveryDisposition::ObservedOnly);
        assert!(candidate.refusal.is_none());
        assert!(!candidate.capabilities.enrollment_eligible);
        assert!(!candidate.capabilities.service_control_contract);
        assert!(!candidate.capabilities.user_session_control_contract);
    }

    #[test]
    fn missing_identity_evidence_is_refused() {
        let mut fact = service_fact(
            "fixture.scm.generic-missing",
            r"C:\fixture\runners\missing",
            repo_scope("fixture-owner", "repo-missing"),
            3,
            DiscoveredLifecycle::Listening,
        );
        fact.execution_identity_ref = None;
        let report = inventory(vec![fact], vec![], vec![]).discover();
        assert_eq!(
            report.candidates[0].refusal,
            Some(DiscoveryRefusalReason::IdentityEvidenceMissing)
        );
        assert!(!report.candidates[0].capabilities.enrollment_eligible);
    }

    #[test]
    fn incomplete_server_identity_is_observed_but_not_enrollment_eligible() {
        // Without an expected-identity filter, incomplete server identity remains
        // visible as ObservedOnly but must not be enrollment-eligible.
        let mut fact = service_fact(
            "fixture.scm.generic-incomplete",
            r"C:\fixture\runners\incomplete",
            repo_scope("fixture-owner", "repo-incomplete"),
            3,
            DiscoveredLifecycle::Listening,
        );
        fact.runner_id = None;
        let report = RunnerDiscoveryInventory::new(
            SyntheticServiceDiscoverySource::new(vec![fact]),
            SyntheticSessionDiscoverySource::new(vec![]),
            SyntheticUnsupportedDiscoverySource::new(vec![]),
            DiscoveryIdentityFilter::default(),
        )
        .discover();
        assert_eq!(
            report.candidates[0].disposition,
            DiscoveryDisposition::ObservedOnly
        );
        assert!(report.candidates[0].refusal.is_none());
        assert!(!report.candidates[0].capabilities.enrollment_eligible);
    }

    #[test]
    fn unknown_session_without_process_evidence_stays_unknown() {
        let report = inventory(
            vec![],
            vec![session_fact(
                r"C:\fixture\runners\unknown-session",
                repo_scope("fixture-owner", "repo-unknown-session"),
                66,
                true,
                DiscoveredLifecycle::Unknown,
            )],
            vec![],
        )
        .discover();
        let candidate = &report.candidates[0];
        assert_eq!(candidate.observed_lifecycle, DiscoveredLifecycle::Unknown);
        assert_ne!(candidate.observed_lifecycle, DiscoveredLifecycle::Stopped);
    }

    #[test]
    fn read_only_boundary_is_enforced_through_discovery_helper() {
        type Inventory = RunnerDiscoveryInventory<
            SyntheticServiceDiscoverySource,
            SyntheticSessionDiscoverySource,
            SyntheticUnsupportedDiscoverySource,
        >;
        // Exercise the helper rather than asserting bare associated constants
        // (clippy::assertions_on_constants).
        assert!(super::read_only_boundary::discovery_is_read_only::<Inventory>());
        let report = inventory(
            vec![service_fact(
                "fixture.scm.generic-boundary",
                r"C:\fixture\runners\boundary",
                repo_scope("fixture-owner", "repo-boundary"),
                1,
                DiscoveredLifecycle::Stopped,
            )],
            vec![],
            vec![],
        )
        .discover();
        assert!(report.read_only);
        assert!(report.candidates[0].capabilities.service_control_contract);
        // Contract availability is not execution authority in LRM-01.
    }

    #[test]
    fn discovery_keys_are_stable_opaque_and_not_process_ids() {
        let home = r"C:\fixture\runners\key";
        let report = inventory(
            vec![service_fact(
                "fixture.scm.generic-key",
                home,
                repo_scope("fixture-owner", "repo-key"),
                12,
                DiscoveredLifecycle::Listening,
            )],
            vec![],
            vec![],
        )
        .discover();
        let key = &report.candidates[0].local_discovery_key;
        let again = discovery_key(
            RunnerBackendKind::WindowsService,
            Path::new(home),
            Some("fixture.scm.generic-key"),
            None,
        );
        assert_eq!(key, &again);
        assert!(key.starts_with("windows-service|home:"));
        assert!(!key.contains('\\'));
        assert!(!key.to_ascii_lowercase().contains("fixture"));
        assert!(!key.to_ascii_lowercase().contains("runners"));
        assert!(!key.chars().all(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn empty_sources_compose_without_candidates() {
        let report = RunnerDiscoveryInventory::new(
            EmptyServiceDiscoverySource,
            EmptySessionDiscoverySource,
            EmptyUnsupportedDiscoverySource,
            DiscoveryIdentityFilter::default(),
        )
        .discover();
        assert!(report.candidates.is_empty());
        assert!(report.read_only);
        assert!(!report.exhaustive);
        assert_eq!(report.service_enumeration, DiscoveryEnumerationStatus::Ok);
    }

    #[test]
    fn nonempty_synthetic_inventory_separates_two_candidates_and_collisions() {
        let report = inventory(
            vec![
                service_fact(
                    "fixture.scm.a",
                    r"C:\fixture\runners\a",
                    repo_scope("fixture-owner", "repo-a"),
                    1,
                    DiscoveredLifecycle::Listening,
                ),
                service_fact(
                    "fixture.scm.b",
                    r"C:\fixture\runners\b",
                    repo_scope("fixture-owner", "repo-b"),
                    2,
                    DiscoveredLifecycle::Stopped,
                ),
            ],
            vec![session_fact(
                r"C:\fixture\runners\a",
                repo_scope("fixture-owner", "repo-a"),
                1,
                true,
                DiscoveredLifecycle::Unknown,
            )],
            Vec::new(),
        )
        .discover();
        assert!(report.candidates.len() >= 2);
        // Duplicate home between service+session must refuse at least one.
        assert!(report.refused().count() >= 1);
        // Service Listening must not imply Worker Busy without process evidence.
        let listening = report.candidates.iter().find(|c| {
            c.service
                .as_ref()
                .is_some_and(|s| s.state == DiscoveredLifecycle::Listening)
        });
        if let Some(candidate) = listening {
            assert_ne!(candidate.observed_lifecycle, DiscoveredLifecycle::Busy);
        }
    }

    #[test]
    fn unavailable_service_enumeration_marks_incomplete_not_false_empty_success() {
        #[derive(Clone, Default)]
        struct UnavailableServices;
        impl ServiceDiscoverySource for UnavailableServices {
            fn list_service_candidates(&self) -> Vec<ServiceCandidateFact> {
                Vec::new()
            }
            fn enumeration_status(&self) -> DiscoveryEnumerationStatus {
                DiscoveryEnumerationStatus::Unavailable
            }
        }
        let report = RunnerDiscoveryInventory::new(
            UnavailableServices,
            EmptySessionDiscoverySource,
            EmptyUnsupportedDiscoverySource,
            DiscoveryIdentityFilter::default(),
        )
        .discover();
        assert!(report.candidates.is_empty());
        assert!(report.is_incomplete());
        assert!(report.is_false_empty_success());
    }

    #[test]
    fn process_observation_unavailable_marks_incomplete_without_false_empty_service_ok() {
        let report = RunnerDiscoveryInventory::new(
            EmptyServiceDiscoverySource,
            EmptySessionDiscoverySource,
            EmptyUnsupportedDiscoverySource,
            DiscoveryIdentityFilter::default(),
        )
        .discover_with_process_status(DiscoveryEnumerationStatus::Unavailable);
        assert!(report.candidates.is_empty());
        assert!(report.is_incomplete());
        assert_eq!(report.service_enumeration, DiscoveryEnumerationStatus::Ok);
        // Service channel Ok + empty must not be rewritten as Unavailable false-empty.
        assert!(!report.is_false_empty_success());
        assert_eq!(
            report.process_observation,
            DiscoveryEnumerationStatus::Unavailable
        );
    }

    #[test]
    fn incomplete_service_enumeration_with_partial_candidates_stays_incomplete() {
        #[derive(Clone, Default)]
        struct IncompleteServices;
        impl ServiceDiscoverySource for IncompleteServices {
            fn list_service_candidates(&self) -> Vec<ServiceCandidateFact> {
                vec![service_fact(
                    "fixture.scm.partial",
                    r"C:\fixture\runners\partial",
                    repo_scope("fixture-owner", "repo-partial"),
                    9,
                    DiscoveredLifecycle::Listening,
                )]
            }
            fn enumeration_status(&self) -> DiscoveryEnumerationStatus {
                DiscoveryEnumerationStatus::Incomplete
            }
        }
        let report = RunnerDiscoveryInventory::new(
            IncompleteServices,
            EmptySessionDiscoverySource,
            EmptyUnsupportedDiscoverySource,
            DiscoveryIdentityFilter::default(),
        )
        .discover();
        assert_eq!(report.candidates.len(), 1);
        assert!(report.is_incomplete());
        assert!(!report.is_false_empty_success());
        // Partial failure must not auto-enroll through discovery alone.
        assert_eq!(
            report.candidates[0].disposition,
            DiscoveryDisposition::ObservedOnly
        );
    }
}
