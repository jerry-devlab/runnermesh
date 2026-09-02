//! Sandboxed update staging, activation and deterministic rollback recovery.
//!
//! The coordinator consumes a typed safe-point observation; it never observes,
//! schedules, signals or terminates a GitHub runner Worker.

use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    installation::{atomic_write, is_reparse_point},
    InstallError, Installation,
};

pub const UPDATE_SCHEMA_VERSION: u32 = 1;
const TRANSACTION_FILE: &str = "update-transaction.json";
const RECEIPT_FILE: &str = "update-receipt.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateRequest {
    pub version: String,
    pub payload: PathBuf,
    pub expected_payload_sha256: String,
    pub compatible: bool,
}

impl UpdateRequest {
    pub fn new(
        version: impl Into<String>,
        payload: impl Into<PathBuf>,
        expected_payload_sha256: impl Into<String>,
        compatible: bool,
    ) -> Self {
        Self {
            version: version.into(),
            payload: payload.into(),
            expected_payload_sha256: expected_payload_sha256.into(),
            compatible,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdatePhase {
    Intent,
    ReadyToActivate,
    DeferredForActiveJob,
    Switched,
    Committed,
    RolledBack,
}

impl UpdatePhase {
    fn terminal(self) -> bool {
        matches!(self, Self::Committed | Self::RolledBack)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTransaction {
    pub schema_version: u32,
    pub transaction_id: String,
    pub previous_version: String,
    pub requested_version: String,
    pub payload_sha256: String,
    pub phase: UpdatePhase,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdateOutcome {
    Committed,
    RolledBack,
    DeferredForActiveJob,
    RecoveredRollback,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateReceipt {
    pub schema_version: u32,
    pub transaction_id: String,
    pub outcome: UpdateOutcome,
    pub active_version: String,
    pub poststate_reconciled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcileReceipt {
    pub transaction_id: String,
    pub outcome: UpdateOutcome,
    pub active_version: String,
}

/// The update layer's only active-work input. `Unknown` fails closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafePointObservation {
    Idle,
    ActiveBoundWork,
    Unknown,
}

/// A supplied health result; this source component does not launch a process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthObservation {
    Healthy,
    Unhealthy(String),
    Unknown,
}

#[derive(Clone, Debug)]
pub struct UpdateCoordinator {
    installation: Installation,
}

impl UpdateCoordinator {
    pub fn new(installation: Installation) -> Self {
        Self { installation }
    }

    pub fn installation(&self) -> &Installation {
        &self.installation
    }

    pub fn transaction(&self) -> Result<Option<UpdateTransaction>, UpdateError> {
        self.read_json(self.transaction_path())
    }

    pub fn receipt(&self) -> Result<Option<UpdateReceipt>, UpdateError> {
        self.read_json(self.receipt_path())
    }

    /// `INTENT` is persisted before the immutable slot is written; only a
    /// verified slot advances the durable journal to `READY_TO_ACTIVATE`.
    pub fn stage(&self, request: &UpdateRequest) -> Result<UpdateTransaction, UpdateError> {
        validate_update_version(&request.version)?;
        if !request.compatible {
            return Err(UpdateError::Incompatible);
        }
        let actual = payload_sha256(&request.payload)?;
        if actual != request.expected_payload_sha256 {
            return Err(UpdateError::ChecksumMismatch {
                expected: request.expected_payload_sha256.clone(),
                actual,
            });
        }
        self.ensure_no_inflight_transaction()?;
        let state = self.installation.state()?;
        let previous = state.active_version.ok_or(UpdateError::NoPreviousVersion)?;
        if previous == request.version {
            return Err(UpdateError::SameVersion(request.version.clone()));
        }
        if state.versions.contains_key(&request.version) {
            return Err(UpdateError::VersionAlreadyInstalled(
                request.version.clone(),
            ));
        }
        let mut transaction = UpdateTransaction {
            schema_version: UPDATE_SCHEMA_VERSION,
            transaction_id: unique_transaction_id(),
            previous_version: previous,
            requested_version: request.version.clone(),
            payload_sha256: actual,
            phase: UpdatePhase::Intent,
        };
        self.write_transaction(&transaction)?;
        self.installation
            .install(&request.version, &request.payload)?;
        transaction.phase = UpdatePhase::ReadyToActivate;
        self.write_transaction(&transaction)?;
        Ok(transaction)
    }

    /// Activates only at a supplied idle safe point. An active job produces a
    /// durable deferral and no process-control action of any kind.
    pub fn activate(
        &self,
        safe_point: SafePointObservation,
        health: HealthObservation,
    ) -> Result<UpdateReceipt, UpdateError> {
        let mut transaction = self.transaction()?.ok_or(UpdateError::NoTransaction)?;
        if !matches!(
            transaction.phase,
            UpdatePhase::ReadyToActivate | UpdatePhase::DeferredForActiveJob
        ) {
            return Err(UpdateError::UnexpectedPhase(transaction.phase));
        }
        match safe_point {
            SafePointObservation::Unknown => return Err(UpdateError::SafePointUnknown),
            SafePointObservation::ActiveBoundWork => {
                transaction.phase = UpdatePhase::DeferredForActiveJob;
                self.write_transaction(&transaction)?;
                return self.write_receipt(
                    &transaction,
                    UpdateOutcome::DeferredForActiveJob,
                    &transaction.previous_version,
                );
            }
            SafePointObservation::Idle => {}
        }
        if transaction.phase == UpdatePhase::DeferredForActiveJob {
            transaction.phase = UpdatePhase::ReadyToActivate;
            self.write_transaction(&transaction)?;
        }
        self.installation
            .select_active(&transaction.requested_version)?;
        transaction.phase = UpdatePhase::Switched;
        self.write_transaction(&transaction)?;
        if health != HealthObservation::Healthy {
            self.installation
                .select_active(&transaction.previous_version)?;
            transaction.phase = UpdatePhase::RolledBack;
            self.write_transaction(&transaction)?;
            return self.write_receipt(
                &transaction,
                UpdateOutcome::RolledBack,
                &transaction.previous_version,
            );
        }
        transaction.phase = UpdatePhase::Committed;
        self.write_transaction(&transaction)?;
        self.write_receipt(
            &transaction,
            UpdateOutcome::Committed,
            &transaction.requested_version,
        )
    }

    /// A nonterminal journal is an interruption. Reconciliation returns to the
    /// known previous slot, but refuses third-party active-version drift or a
    /// damaged/missing owned slot.
    pub fn reconcile(&self) -> Result<ReconcileReceipt, UpdateError> {
        let mut transaction = self.transaction()?.ok_or(UpdateError::NoTransaction)?;
        let current = self.active_version()?;
        if transaction.phase.terminal() {
            let expected = if transaction.phase == UpdatePhase::Committed {
                &transaction.requested_version
            } else {
                &transaction.previous_version
            };
            if &current != expected {
                return Err(UpdateError::ActiveVersionDrift {
                    expected: expected.clone(),
                    actual: current,
                });
            }
            let outcome = if transaction.phase == UpdatePhase::Committed {
                UpdateOutcome::Committed
            } else {
                UpdateOutcome::RolledBack
            };
            self.ensure_terminal_receipt(&transaction, outcome, expected)?;
            return Ok(ReconcileReceipt {
                transaction_id: transaction.transaction_id,
                outcome,
                active_version: expected.clone(),
            });
        }
        if current != transaction.previous_version && current != transaction.requested_version {
            return Err(UpdateError::ActiveVersionDrift {
                expected: transaction.previous_version,
                actual: current,
            });
        }
        if current != transaction.previous_version {
            self.installation
                .select_active(&transaction.previous_version)?;
        }
        transaction.phase = UpdatePhase::RolledBack;
        self.write_transaction(&transaction)?;
        self.write_receipt(
            &transaction,
            UpdateOutcome::RecoveredRollback,
            &transaction.previous_version,
        )?;
        Ok(ReconcileReceipt {
            transaction_id: transaction.transaction_id,
            outcome: UpdateOutcome::RecoveredRollback,
            active_version: transaction.previous_version,
        })
    }

    pub fn clear_terminal_journal(&self) -> Result<bool, UpdateError> {
        let Some(transaction) = self.transaction()? else {
            return Ok(false);
        };
        if !transaction.phase.terminal() {
            return Err(UpdateError::UnreconciledTransaction(transaction.phase));
        }
        for path in [self.transaction_path(), self.receipt_path()] {
            if is_reparse_point(&path)
                .map_err(|error| UpdateError::DamagedJournal(error.to_string()))?
            {
                return Err(UpdateError::DamagedJournal(format!(
                    "refusing reparse journal {}",
                    path.display()
                )));
            }
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(UpdateError::io(error)),
            }
        }
        Ok(true)
    }

    fn ensure_no_inflight_transaction(&self) -> Result<(), UpdateError> {
        if let Some(transaction) = self.transaction()? {
            if !transaction.phase.terminal() {
                return Err(UpdateError::UnreconciledTransaction(transaction.phase));
            }
        }
        Ok(())
    }

    fn active_version(&self) -> Result<String, UpdateError> {
        self.installation
            .state()?
            .active_version
            .ok_or(UpdateError::NoPreviousVersion)
    }

    fn transaction_path(&self) -> PathBuf {
        self.installation.state_dir().join(TRANSACTION_FILE)
    }

    fn receipt_path(&self) -> PathBuf {
        self.installation.state_dir().join(RECEIPT_FILE)
    }

    fn read_json<T>(&self, path: PathBuf) -> Result<Option<T>, UpdateError>
    where
        T: for<'a> Deserialize<'a>,
    {
        if is_reparse_point(&path)
            .map_err(|error| UpdateError::DamagedJournal(error.to_string()))?
        {
            return Err(UpdateError::DamagedJournal(format!(
                "refusing reparse journal {}",
                path.display()
            )));
        }
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| UpdateError::DamagedJournal(error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(UpdateError::io(error)),
        }
    }

    fn write_transaction(&self, transaction: &UpdateTransaction) -> Result<(), UpdateError> {
        if transaction.schema_version != UPDATE_SCHEMA_VERSION {
            return Err(UpdateError::DamagedJournal(
                "unsupported transaction schema".to_owned(),
            ));
        }
        self.write_json(self.transaction_path(), transaction)
    }

    fn write_receipt(
        &self,
        transaction: &UpdateTransaction,
        outcome: UpdateOutcome,
        active_version: &str,
    ) -> Result<UpdateReceipt, UpdateError> {
        let receipt = UpdateReceipt {
            schema_version: UPDATE_SCHEMA_VERSION,
            transaction_id: transaction.transaction_id.clone(),
            outcome,
            active_version: active_version.to_owned(),
            poststate_reconciled: true,
        };
        self.write_json(self.receipt_path(), &receipt)?;
        Ok(receipt)
    }

    fn ensure_terminal_receipt(
        &self,
        transaction: &UpdateTransaction,
        outcome: UpdateOutcome,
        active_version: &str,
    ) -> Result<(), UpdateError> {
        if let Some(receipt) = self.receipt()? {
            if receipt.transaction_id == transaction.transaction_id
                && receipt.outcome == outcome
                && receipt.active_version == active_version
                && receipt.poststate_reconciled
            {
                return Ok(());
            }
        }
        self.write_receipt(transaction, outcome, active_version)
            .map(|_| ())
    }

    fn write_json<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<(), UpdateError> {
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|error| UpdateError::DamagedJournal(error.to_string()))?;
        atomic_write(&path, &bytes).map_err(UpdateError::io)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum UpdateError {
    Installation(InstallError),
    Io(String),
    ChecksumMismatch { expected: String, actual: String },
    Incompatible,
    NoPreviousVersion,
    SameVersion(String),
    VersionAlreadyInstalled(String),
    NoTransaction,
    UnexpectedPhase(UpdatePhase),
    UnreconciledTransaction(UpdatePhase),
    ActiveVersionDrift { expected: String, actual: String },
    HealthCheckFailed(String),
    SafePointUnknown,
    DamagedJournal(String),
    InvalidPayload(String),
}

impl UpdateError {
    fn io(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<InstallError> for UpdateError {
    fn from(error: InstallError) -> Self {
        Self::Installation(error)
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Installation(error) => write!(formatter, "installation failed: {error}"),
            Self::Io(error) => write!(formatter, "update I/O failed: {error}"),
            Self::ChecksumMismatch { expected, actual } => write!(
                formatter,
                "checksum mismatch: expected {expected}, got {actual}"
            ),
            Self::Incompatible => formatter.write_str("candidate is incompatible"),
            Self::NoPreviousVersion => formatter.write_str("no prior active version"),
            Self::SameVersion(version) => write!(formatter, "version already active: {version}"),
            Self::VersionAlreadyInstalled(version) => {
                write!(formatter, "version already installed: {version}")
            }
            Self::NoTransaction => formatter.write_str("no update transaction exists"),
            Self::UnexpectedPhase(phase) => write!(formatter, "operation not valid in {phase:?}"),
            Self::UnreconciledTransaction(phase) => write!(
                formatter,
                "transaction requires reconciliation from {phase:?}"
            ),
            Self::ActiveVersionDrift { expected, actual } => write!(
                formatter,
                "active version drift: expected {expected}, got {actual}"
            ),
            Self::HealthCheckFailed(reason) => {
                write!(formatter, "candidate health failed: {reason}")
            }
            Self::SafePointUnknown => formatter.write_str("safe point is unknown"),
            Self::DamagedJournal(reason) => write!(formatter, "damaged update journal: {reason}"),
            Self::InvalidPayload(reason) => write!(formatter, "invalid update payload: {reason}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Hashes a directory in stable relative-path order and rejects links,
/// traversal and non-file entries rather than dereferencing them.
pub fn payload_sha256(payload: &Path) -> Result<String, UpdateError> {
    if !payload.is_dir() {
        return Err(UpdateError::InvalidPayload(format!(
            "not a directory: {}",
            payload.display()
        )));
    }
    let mut files = BTreeMap::new();
    collect_payload_hashes(payload, payload, &mut files)?;
    if files.is_empty() {
        return Err(UpdateError::InvalidPayload("payload is empty".to_owned()));
    }
    let mut hasher = Sha256::new();
    for (path, digest) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(digest.as_bytes());
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_payload_hashes(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), UpdateError> {
    for entry in fs::read_dir(current).map_err(UpdateError::io)? {
        let entry = entry.map_err(UpdateError::io)?;
        let path = entry.path();
        if entry.file_type().map_err(UpdateError::io)?.is_symlink() {
            return Err(UpdateError::InvalidPayload(format!(
                "link refused: {}",
                path.display()
            )));
        }
        if path.is_dir() {
            collect_payload_hashes(root, &path, files)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| UpdateError::InvalidPayload(error.to_string()))?;
            let key = relative_key(relative)?;
            files.insert(key, sha256_path(&path)?);
        } else {
            return Err(UpdateError::InvalidPayload(format!(
                "unsupported entry: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn relative_key(path: &Path) -> Result<String, UpdateError> {
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(UpdateError::InvalidPayload(path.display().to_string()));
    }
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| UpdateError::InvalidPayload(path.display().to_string()))
}

fn validate_update_version(version: &str) -> Result<(), UpdateError> {
    if version.is_empty()
        || version.len() > 128
        || version.starts_with('.')
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(UpdateError::InvalidPayload(
            "invalid requested version".to_owned(),
        ));
    }
    Ok(())
}

fn sha256_path(path: &Path) -> Result<String, UpdateError> {
    let mut file = fs::File::open(path).map_err(UpdateError::io)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(UpdateError::io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn unique_transaction_id() -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("update-{suffix:032x}")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{payload_sha256, HealthObservation, UPDATE_SCHEMA_VERSION};
    use crate::{
        Installation, SafePointObservation, UpdateCoordinator, UpdateError, UpdateOutcome,
        UpdatePhase, UpdateRequest, UpdateTransaction,
    };

    fn root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "runnermesh-update-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn payload(root: &Path, name: &str) -> PathBuf {
        let path = root.join(format!("payload-{name}"));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("runnermesh.exe"), format!("cli-{name}")).unwrap();
        fs::write(path.join("runnermesh-agent.exe"), format!("agent-{name}")).unwrap();
        path
    }

    fn prepared(name: &str) -> (PathBuf, UpdateCoordinator, PathBuf) {
        let root = root(name);
        let installation = Installation::new(root.join("installed"));
        installation
            .install("0.1.0", &payload(&root, "old"))
            .unwrap();
        let candidate = payload(&root, "new");
        (root, UpdateCoordinator::new(installation), candidate)
    }

    #[test]
    fn stage_is_checksum_compatible_and_durable_before_activation() {
        let (_root, update, payload) = prepared("stage");
        assert_eq!(
            update.stage(&UpdateRequest::new("0.2.0", &payload, "wrong", true)),
            Err(UpdateError::ChecksumMismatch {
                expected: "wrong".to_owned(),
                actual: payload_sha256(&payload).unwrap()
            })
        );
        assert_eq!(
            update.stage(&UpdateRequest::new(
                "0.2.0",
                &payload,
                payload_sha256(&payload).unwrap(),
                false
            )),
            Err(UpdateError::Incompatible)
        );
        assert_eq!(update.transaction().unwrap(), None);
        let transaction = update
            .stage(&UpdateRequest::new(
                "0.2.0",
                &payload,
                payload_sha256(&payload).unwrap(),
                true,
            ))
            .unwrap();
        assert_eq!(transaction.phase, UpdatePhase::ReadyToActivate);
        assert_eq!(
            update.transaction().unwrap().unwrap().phase,
            UpdatePhase::ReadyToActivate
        );
        assert_eq!(
            update
                .installation()
                .state()
                .unwrap()
                .active_version
                .as_deref(),
            Some("0.1.0")
        );
    }

    #[test]
    fn active_work_defers_and_health_failure_rolls_back() {
        let (_root, update, payload) = prepared("activation");
        update
            .stage(&UpdateRequest::new(
                "0.2.0",
                &payload,
                payload_sha256(&payload).unwrap(),
                true,
            ))
            .unwrap();
        assert_eq!(
            update
                .activate(
                    SafePointObservation::ActiveBoundWork,
                    HealthObservation::Healthy
                )
                .unwrap()
                .outcome,
            UpdateOutcome::DeferredForActiveJob
        );
        assert_eq!(
            update.transaction().unwrap().unwrap().phase,
            UpdatePhase::DeferredForActiveJob
        );
        assert_eq!(
            update
                .installation()
                .state()
                .unwrap()
                .active_version
                .as_deref(),
            Some("0.1.0")
        );
        assert_eq!(
            update
                .activate(
                    SafePointObservation::Idle,
                    HealthObservation::Unhealthy("fixture".to_owned())
                )
                .unwrap()
                .outcome,
            UpdateOutcome::RolledBack
        );
        assert_eq!(
            update
                .installation()
                .state()
                .unwrap()
                .active_version
                .as_deref(),
            Some("0.1.0")
        );
    }

    #[test]
    fn interruptions_drift_corruption_and_duplicate_transaction_fail_closed() {
        let (_root, update, payload) = prepared("reconcile");
        update
            .stage(&UpdateRequest::new(
                "0.2.0",
                &payload,
                payload_sha256(&payload).unwrap(),
                true,
            ))
            .unwrap();
        assert!(matches!(
            update.stage(&UpdateRequest::new(
                "0.3.0",
                &payload,
                payload_sha256(&payload).unwrap(),
                true
            )),
            Err(UpdateError::UnreconciledTransaction(_))
        ));
        assert_eq!(
            update.reconcile().unwrap().outcome,
            UpdateOutcome::RecoveredRollback
        );
        assert_eq!(
            update.reconcile().unwrap().outcome,
            UpdateOutcome::RolledBack
        );
        update.clear_terminal_journal().unwrap();
        fs::write(
            update
                .installation()
                .state_dir()
                .join("update-transaction.json"),
            b"{",
        )
        .unwrap();
        assert!(matches!(
            update.reconcile(),
            Err(UpdateError::DamagedJournal(_))
        ));
    }

    #[test]
    fn intent_ready_switched_and_third_party_drift_reconcile_truthfully() {
        let (_root, update, candidate_payload) = prepared("interruption-phases");
        let intent = UpdateTransaction {
            schema_version: UPDATE_SCHEMA_VERSION,
            transaction_id: "fixture-intent".to_owned(),
            previous_version: "0.1.0".to_owned(),
            requested_version: "0.2.0".to_owned(),
            payload_sha256: payload_sha256(&candidate_payload).unwrap(),
            phase: UpdatePhase::Intent,
        };
        fs::write(
            update
                .installation()
                .state_dir()
                .join("update-transaction.json"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        assert_eq!(
            update.reconcile().unwrap().outcome,
            UpdateOutcome::RecoveredRollback
        );
        update.clear_terminal_journal().unwrap();

        update
            .stage(&UpdateRequest::new(
                "0.2.0",
                &candidate_payload,
                payload_sha256(&candidate_payload).unwrap(),
                true,
            ))
            .unwrap();
        update.installation().select_active("0.2.0").unwrap();
        assert_eq!(
            update.reconcile().unwrap().outcome,
            UpdateOutcome::RecoveredRollback
        );

        let (_root, drift, candidate) = prepared("third-party-drift");
        let third = payload(drift.installation().root().parent().unwrap(), "third");
        drift.installation().install("0.3.0", &third).unwrap();
        drift
            .stage(&UpdateRequest::new(
                "0.2.0",
                &candidate,
                payload_sha256(&candidate).unwrap(),
                true,
            ))
            .unwrap();
        drift.installation().select_active("0.3.0").unwrap();
        assert!(matches!(
            drift.reconcile(),
            Err(UpdateError::ActiveVersionDrift { .. })
        ));
    }

    #[test]
    fn unknown_safe_point_and_missing_old_slot_refuse() {
        let (_root, update, payload) = prepared("drift");
        update
            .stage(&UpdateRequest::new(
                "0.2.0",
                &payload,
                payload_sha256(&payload).unwrap(),
                true,
            ))
            .unwrap();
        assert_eq!(
            update.activate(SafePointObservation::Unknown, HealthObservation::Healthy),
            Err(UpdateError::SafePointUnknown)
        );
        fs::remove_dir_all(update.installation().versions_dir().join("0.1.0")).unwrap();
        assert!(matches!(
            update.reconcile(),
            Err(UpdateError::Installation(_))
        ));
    }
}
