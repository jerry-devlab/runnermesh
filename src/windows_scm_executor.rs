//! Windows SCM control executor for future HUX-2 (mutation arm sealed closed).
//!
//! Authorization model:
//! - [`SyntheticScmTestGrant`] opens a gate usable **only** with mock/synthetic
//!   flows. It is forgeable by design and must never be treated as Owner auth.
//! - [`TrustedOwnerAuthorizationVerifier`] is the sole live-auth entry. This
//!   Train ships [`UnissuedOwnerAuthorizationVerifier`], which always refuses.
//!
//! Win32 `QueryServiceStatusEx(SC_STATUS_PROCESS_INFO)` returns
//! [`SERVICE_STATUS_PROCESS`], not `ENUM_SERVICE_STATUS_PROCESSW`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

use crate::{
    achieved_from_observation, capture_restore_plan, next_control_phase_with_admission,
    AchievedServiceControl, DesiredServiceLifecycle, DiscoveredLifecycle, ServiceControlPhase,
    ServiceObservation, ServicePrestateCapture, ServiceRestorePlan, ServiceStartMode,
};

/// Independent GitHub admission selector observation used by restore verification.
///
/// Callers must not pass a bare boolean as accepted fact; the observer performs
/// a fresh read (fake or live) at verification time.
pub trait AdmissionSelectorObserver {
    fn observe_selector_absent(&mut self) -> Result<bool, ScmApiError>;
}

/// Authorized scope defining exact target and rights bound by Owner authorization.
///
/// Implements P03 multi-dimensional binding preventing authorization confusion:
/// 1. `service_name`: exact Windows service name (no wildcards)
/// 2. `image_token`: opaque token bound to service executable path
/// 3. `execution_identity_token`: opaque token bound to service account / user identity
/// 4. `target_host_token`: opaque token bound to the authorized host machine
/// 5. `candidate_commit`: exact 40-character git commit SHA of candidate under review
/// 6. `transaction_id`: unique bounded transaction identifier
/// 7. `allow_start` / `allow_stop` / `allow_change_config`: rights bitset
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerAuthorizationScope {
    pub service_name: String,
    pub image_token: String,
    pub execution_identity_token: String,
    pub target_host_token: String,
    pub candidate_commit: String,
    pub transaction_id: String,
    pub allow_start: bool,
    pub allow_stop: bool,
    pub allow_change_config: bool,
}

impl OwnerAuthorizationScope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service_name: impl Into<String>,
        image_token: impl Into<String>,
        execution_identity_token: impl Into<String>,
        target_host_token: impl Into<String>,
        candidate_commit: impl Into<String>,
        transaction_id: impl Into<String>,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            image_token: image_token.into(),
            execution_identity_token: execution_identity_token.into(),
            target_host_token: target_host_token.into(),
            candidate_commit: candidate_commit.into(),
            transaction_id: transaction_id.into(),
            allow_start,
            allow_stop,
            allow_change_config,
        }
    }

    #[cfg(test)]
    pub fn test_fixture(service_name: &str, image_token: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
            image_token: image_token.to_string(),
            execution_identity_token: "fixture-identity".to_string(),
            target_host_token: "fixture-host-token".to_string(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_string(),
            transaction_id: "tx-fixture-1".to_string(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: false,
        }
    }
}

/// Request presented to a trusted Owner authorization verifier.
///
/// Implements the P03 trusted SCM authorization contract binding all 9 dimensions:
/// 1. Exact SCM service name (`service_name`)
/// 2. Opaque image token (`image_token`)
/// 3. Opaque execution identity token (`execution_identity_token`)
/// 4. Target host token (`target_host_token`)
/// 5. Candidate commit SHA (`candidate_commit`)
/// 6. Permissions bitset (`allow_start`, `allow_stop`, `allow_change_config`)
/// 7. Bounded transaction ID (`transaction_id`)
/// 8. Validity window (`not_before_epoch_seconds`, `expires_at_epoch_seconds`)
/// 9. Single-use nonce (`nonce`)
#[derive(Clone, Eq, PartialEq)]
pub struct TrustedOwnerAuthorizationRequest {
    pub service_name: String,
    pub image_token: String,
    pub execution_identity_token: String,
    pub target_host_token: String,
    pub candidate_commit: String,
    pub allow_start: bool,
    pub allow_stop: bool,
    pub allow_change_config: bool,
    pub transaction_id: String,
    /// Unix epoch seconds after which this authorization is valid.
    pub not_before_epoch_seconds: u64,
    /// Unix epoch seconds after which this authorization is expired.
    pub expires_at_epoch_seconds: u64,
    /// Single-use nonce preventing replay.
    pub nonce: String,
    /// Opaque cryptographic Owner voucher material (never logged).
    pub owner_material: Vec<u8>,
}

impl std::fmt::Debug for TrustedOwnerAuthorizationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustedOwnerAuthorizationRequest")
            .field("service_name", &self.service_name)
            .field("image_token", &self.image_token)
            .field("execution_identity_token", &self.execution_identity_token)
            .field("target_host_token", &self.target_host_token)
            .field("candidate_commit", &self.candidate_commit)
            .field("allow_start", &self.allow_start)
            .field("allow_stop", &self.allow_stop)
            .field("allow_change_config", &self.allow_change_config)
            .field("transaction_id", &self.transaction_id)
            .field("not_before_epoch_seconds", &self.not_before_epoch_seconds)
            .field("expires_at_epoch_seconds", &self.expires_at_epoch_seconds)
            .field("nonce", &self.nonce)
            .field("owner_material", &"[REDACTED]")
            .finish()
    }
}

impl TrustedOwnerAuthorizationRequest {
    pub fn new_test_fixture(
        service_name: impl Into<String>,
        image_token: impl Into<String>,
        owner_material: Vec<u8>,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            image_token: image_token.into(),
            execution_identity_token: "test-execution-identity".into(),
            target_host_token: "test-host-token".into(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".into(),
            allow_start,
            allow_stop,
            allow_change_config,
            transaction_id: "tx-test-fixture".into(),
            owner_material,
            not_before_epoch_seconds: 1_700_000_000,
            expires_at_epoch_seconds: 1_800_000_000,
            nonce: "default-test-nonce".to_owned(),
        }
    }
}

/// Clock abstraction for trusted boundary authorization verification.
pub trait TrustedClock: Send + Sync + std::fmt::Debug {
    fn now_epoch_seconds(&self) -> u64;
}

/// Default system clock using [`SystemTime::now()`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl TrustedClock for SystemClock {
    fn now_epoch_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

/// Controllable mock clock for deterministic testing.
#[derive(Clone, Debug)]
pub struct MockClock {
    now: Arc<Mutex<u64>>,
}

impl MockClock {
    pub fn new(initial_time: u64) -> Self {
        Self {
            now: Arc::new(Mutex::new(initial_time)),
        }
    }

    pub fn set_time(&self, time: u64) {
        *self.now.lock().unwrap_or_else(|p| p.into_inner()) = time;
    }
}

impl TrustedClock for MockClock {
    fn now_epoch_seconds(&self) -> u64 {
        *self.now.lock().unwrap_or_else(|p| p.into_inner())
    }
}

static GLOBAL_AUTH_LEDGER: std::sync::OnceLock<SharedAuthorizationLedger> =
    std::sync::OnceLock::new();

fn decode_hex_32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

#[cfg(windows)]
pub struct ProcessFileLock {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl ProcessFileLock {
    pub fn acquire(lock_path: &Path) -> io::Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        if let Some(parent) = lock_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut wide: Vec<u16> = lock_path.as_os_str().encode_wide().collect();
        wide.push(0);
        unsafe {
            let handle = windows_sys::Win32::Storage::FileSystem::CreateFileW(
                wide.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ
                    | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE,
                windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ
                    | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                windows_sys::Win32::Storage::FileSystem::OPEN_ALWAYS,
                windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            );
            if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED = std::mem::zeroed();
            let ok = windows_sys::Win32::Storage::FileSystem::LockFileEx(
                handle,
                windows_sys::Win32::Storage::FileSystem::LOCKFILE_EXCLUSIVE_LOCK,
                0,
                1,
                0,
                &mut overlapped,
            );
            if ok == 0 {
                let err = io::Error::last_os_error();
                windows_sys::Win32::Foundation::CloseHandle(handle);
                return Err(err);
            }
            Ok(Self { handle })
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessFileLock {
    fn drop(&mut self) {
        unsafe {
            let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED = std::mem::zeroed();
            windows_sys::Win32::Storage::FileSystem::UnlockFileEx(
                self.handle,
                0,
                1,
                0,
                &mut overlapped,
            );
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(not(windows))]
pub struct ProcessFileLock {
    _file: fs::File,
}

#[cfg(not(windows))]
impl ProcessFileLock {
    pub fn acquire(lock_path: &Path) -> io::Result<Self> {
        if let Some(parent) = lock_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { _file: file })
    }
}

/// Thread-safe shared authorization ledger managing consumed authorizations.
///
/// Prevents both swapped-nonce replays (reusing the same authorization voucher with
/// a different nonce) and reconstructed-verifier replays (instantiating a fresh
/// verifier instance). Supports persistent durable storage to prevent replays across
/// process restarts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DurableLedgerEntry {
    pub token_hash_hex: String,
    pub nonce: String,
    pub consumed_at_unix_ms: u64,
}

#[derive(Default, Debug)]
struct SharedAuthorizationLedgerState {
    consumed_tokens: std::collections::BTreeSet<[u8; 32]>,
    consumed_nonces: std::collections::BTreeSet<String>,
    durable_path: Option<PathBuf>,
}

/// Global replay protection ledger tracking consumed tokens and nonces across verifiers.
///
/// Prevents nonce-swapping and verifier-reconstruction replay attacks (reconstructing a fresh
/// verifier instance to replay a consumed authorization). checking and recording are performed
/// atomically in a single critical section to eliminate concurrent double-consumption races.
#[derive(Clone, Debug)]
pub struct SharedAuthorizationLedger {
    state: Arc<Mutex<SharedAuthorizationLedgerState>>,
}

impl Default for SharedAuthorizationLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedAuthorizationLedger {
    pub fn global() -> &'static SharedAuthorizationLedger {
        GLOBAL_AUTH_LEDGER.get_or_init(Self::new_internal)
    }

    fn new_internal() -> Self {
        Self {
            state: Arc::new(Mutex::new(SharedAuthorizationLedgerState::default())),
        }
    }

    pub fn new() -> Self {
        Self::new_internal()
    }

    /// Constructs a shared authorization ledger backed by a durable persistent file.
    ///
    /// Reads existing consumed vouchers on startup to prevent cross-process / post-restart replays,
    /// and atomically writes newly consumed records to disk before accepting them.
    pub fn with_durable_file(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let mut consumed_tokens = std::collections::BTreeSet::new();
        let mut consumed_nonces = std::collections::BTreeSet::new();

        if path.exists() {
            if crate::installation::is_reparse_point(&path)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("refusing reparse ledger file {}", path.display()),
                ));
            }
            let bytes = fs::read(&path)?;
            if !bytes.is_empty() {
                let records: Vec<DurableLedgerEntry> = serde_json::from_slice(&bytes)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                for rec in records {
                    if let Some(arr) = decode_hex_32(&rec.token_hash_hex) {
                        consumed_tokens.insert(arr);
                    }
                    consumed_nonces.insert(rec.nonce);
                }
            }
        }

        Ok(Self {
            state: Arc::new(Mutex::new(SharedAuthorizationLedgerState {
                consumed_tokens,
                consumed_nonces,
                durable_path: Some(path),
            })),
        })
    }

    /// Atomically checks and records single-use consumption in a single lock acquisition.
    ///
    /// Returns `true` if neither the token hash nor the nonce was previously consumed
    /// and both are now recorded in the ledger (and synced to durable storage if configured).
    /// Returns `false` if either the token hash or the nonce was already consumed, leaving
    /// the ledger state unmodified.
    pub fn try_consume(&self, token_hash: [u8; 32], nonce: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());

        if let Some(ref path) = state.durable_path {
            let lock_path = path.with_extension("lock");
            let _lock = match ProcessFileLock::acquire(&lock_path) {
                Ok(l) => l,
                Err(_) => return false,
            };

            // 1. Re-read on-disk entries inside the cross-process lock
            let mut entries: Vec<DurableLedgerEntry> = Vec::new();
            if path.exists() {
                if crate::installation::is_reparse_point(path).unwrap_or(true) {
                    return false;
                }
                match fs::read(path) {
                    Ok(b) if !b.is_empty() => {
                        match serde_json::from_slice::<Vec<DurableLedgerEntry>>(&b) {
                            Ok(recs) => entries = recs,
                            Err(_) => {
                                // Corrupt JSON must fail closed — NEVER reset bad ledger to empty!
                                return false;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }

            // 2. Check on-disk entries before in-memory state
            let hex_hash: String = token_hash.iter().map(|b| format!("{b:02x}")).collect();
            for entry in &entries {
                if entry.token_hash_hex.eq_ignore_ascii_case(&hex_hash) || entry.nonce == nonce {
                    if let Some(arr) = decode_hex_32(&entry.token_hash_hex) {
                        state.consumed_tokens.insert(arr);
                    }
                    state.consumed_nonces.insert(entry.nonce.clone());
                    return false;
                }
            }

            // 3. Check in-memory state
            if state.consumed_tokens.contains(&token_hash) || state.consumed_nonces.contains(nonce)
            {
                return false;
            }

            // 4. Record and atomically write
            let consumed_at_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            entries.push(DurableLedgerEntry {
                token_hash_hex: hex_hash,
                nonce: nonce.to_string(),
                consumed_at_unix_ms,
            });

            let Ok(serialized) = serde_json::to_vec_pretty(&entries) else {
                return false;
            };
            if crate::installation::atomic_write(path, &serialized).is_err() {
                return false;
            }

            state.consumed_tokens.insert(token_hash);
            state.consumed_nonces.insert(nonce.to_string());
            true
        } else {
            if state.consumed_tokens.contains(&token_hash) || state.consumed_nonces.contains(nonce)
            {
                return false;
            }
            state.consumed_tokens.insert(token_hash);
            state.consumed_nonces.insert(nonce.to_string());
            true
        }
    }

    pub fn is_consumed(&self, token_hash: &[u8; 32], nonce: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.consumed_tokens.contains(token_hash) || state.consumed_nonces.contains(nonce) {
            return true;
        }
        if let Some(ref path) = state.durable_path {
            if path.exists() {
                let lock_path = path.with_extension("lock");
                if let Ok(_lock) = ProcessFileLock::acquire(&lock_path) {
                    if let Ok(b) = fs::read(path) {
                        if let Ok(entries) = serde_json::from_slice::<Vec<DurableLedgerEntry>>(&b) {
                            let hex_hash: String =
                                token_hash.iter().map(|b| format!("{b:02x}")).collect();
                            for entry in entries {
                                if let Some(arr) = decode_hex_32(&entry.token_hash_hex) {
                                    state.consumed_tokens.insert(arr);
                                }
                                state.consumed_nonces.insert(entry.nonce.clone());
                                if entry.token_hash_hex.eq_ignore_ascii_case(&hex_hash)
                                    || entry.nonce == nonce
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub fn record_consumption(&self, token_hash: [u8; 32], nonce: String) -> bool {
        self.try_consume(token_hash, &nonce)
    }

    pub fn consumed_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .consumed_tokens
            .len()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn clear_for_test(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.consumed_tokens.clear();
        state.consumed_nonces.clear();
    }
}

/// Trusted boundary Owner authorization issuer.
///
/// In production, Owner vouchers are issued solely by an authorized control
/// plane possessing the Owner root authority secret. Callers cannot self-select
/// tokens or manufacture valid vouchers without this authority.
#[derive(Clone)]
pub(crate) struct OwnerAuthorizationIssuer {
    authority_secret: Vec<u8>,
    #[allow(dead_code)]
    clock: Arc<dyn TrustedClock>,
}

impl std::fmt::Debug for OwnerAuthorizationIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerAuthorizationIssuer")
            .field("authority_secret", &"<redacted>")
            .finish()
    }
}

#[allow(dead_code)]
impl OwnerAuthorizationIssuer {
    pub(crate) fn new(authority_secret: impl Into<Vec<u8>>) -> Self {
        Self {
            authority_secret: authority_secret.into(),
            clock: Arc::new(SystemClock),
        }
    }

    pub(crate) fn with_clock(
        authority_secret: impl Into<Vec<u8>>,
        clock: Arc<dyn TrustedClock>,
    ) -> Self {
        Self {
            authority_secret: authority_secret.into(),
            clock,
        }
    }

    /// Computes cryptographic HMAC-SHA256 voucher binding all 9 P03 dimensions.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_voucher(
        &self,
        service_name: &str,
        image_token: &str,
        execution_identity_token: &str,
        target_host_token: &str,
        candidate_commit: &str,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
        transaction_id: &str,
        not_before: u64,
        expires_at: u64,
        nonce: &str,
    ) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(&self.authority_secret)
            .expect("HMAC-SHA256 supports arbitrary key lengths");
        mac.update(b"RUNNERMESH_OWNER_AUTH_V1\0");
        mac.update(service_name.as_bytes());
        mac.update(b"\0");
        mac.update(image_token.as_bytes());
        mac.update(b"\0");
        mac.update(execution_identity_token.as_bytes());
        mac.update(b"\0");
        mac.update(target_host_token.as_bytes());
        mac.update(b"\0");
        mac.update(candidate_commit.as_bytes());
        mac.update(b"\0");
        mac.update(&[
            allow_start as u8,
            allow_stop as u8,
            allow_change_config as u8,
        ]);
        mac.update(b"\0");
        mac.update(transaction_id.as_bytes());
        mac.update(b"\0");
        mac.update(&not_before.to_le_bytes());
        mac.update(&expires_at.to_le_bytes());
        mac.update(nonce.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    /// Verifies whether two cryptographic vouchers are equal in constant time.
    #[allow(dead_code)]
    pub(crate) fn verify_voucher_constant_time(&self, actual: &[u8], expected: &[u8]) -> bool {
        actual.ct_eq(expected).into()
    }

    /// Issue a signed authorization request for the specified 9-dimension scope.
    pub(crate) fn issue_for_scope(
        &self,
        scope: &OwnerAuthorizationScope,
        validity_duration_seconds: u64,
        nonce: impl Into<String>,
    ) -> (TrustedOwnerAuthorizationRequest, Vec<u8>) {
        let nonce = nonce.into();
        let not_before = self.clock.now_epoch_seconds();
        let expires_at = not_before + validity_duration_seconds;

        let voucher = self.compute_voucher(
            &scope.service_name,
            &scope.image_token,
            &scope.execution_identity_token,
            &scope.target_host_token,
            &scope.candidate_commit,
            scope.allow_start,
            scope.allow_stop,
            scope.allow_change_config,
            &scope.transaction_id,
            not_before,
            expires_at,
            &nonce,
        );

        let req = TrustedOwnerAuthorizationRequest {
            service_name: scope.service_name.clone(),
            image_token: scope.image_token.clone(),
            execution_identity_token: scope.execution_identity_token.clone(),
            target_host_token: scope.target_host_token.clone(),
            candidate_commit: scope.candidate_commit.clone(),
            allow_start: scope.allow_start,
            allow_stop: scope.allow_stop,
            allow_change_config: scope.allow_change_config,
            transaction_id: scope.transaction_id.clone(),
            not_before_epoch_seconds: not_before,
            expires_at_epoch_seconds: expires_at,
            nonce,
            owner_material: voucher.clone(),
        };

        (req, voucher)
    }

    /// Issue a signed authorization request for the specified parameters.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn issue(
        &self,
        service_name: impl Into<String>,
        image_token: impl Into<String>,
        execution_identity_token: impl Into<String>,
        target_host_token: impl Into<String>,
        candidate_commit: impl Into<String>,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
        transaction_id: impl Into<String>,
        validity_duration_seconds: u64,
        nonce: impl Into<String>,
    ) -> (TrustedOwnerAuthorizationRequest, Vec<u8>) {
        let scope = OwnerAuthorizationScope {
            service_name: service_name.into(),
            image_token: image_token.into(),
            execution_identity_token: execution_identity_token.into(),
            target_host_token: target_host_token.into(),
            candidate_commit: candidate_commit.into(),
            transaction_id: transaction_id.into(),
            allow_start,
            allow_stop,
            allow_change_config,
        };
        self.issue_for_scope(&scope, validity_duration_seconds, nonce)
    }
}

/// Live/trusted authorization entry. Implementations must not accept synthetic grants.
pub trait TrustedOwnerAuthorizationVerifier {
    fn verify(
        &self,
        request: &TrustedOwnerAuthorizationRequest,
    ) -> Result<LiveScmExecutionGate, ScmApiError>;
}

/// Default verifier for SOURCE_AND_SANDBOX_ONLY: never issues live rights.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnissuedOwnerAuthorizationVerifier;

impl TrustedOwnerAuthorizationVerifier for UnissuedOwnerAuthorizationVerifier {
    fn verify(
        &self,
        _request: &TrustedOwnerAuthorizationRequest,
    ) -> Result<LiveScmExecutionGate, ScmApiError> {
        Err(ScmApiError::AuthorizationNotIssued)
    }
}

/// Token-validating Owner authorization verifier.
///
/// Implements cryptographic digest-sealed verification of the Owner authorization
/// token against expected target service, image token, execution identity, host token,
/// candidate commit, transaction id, and granular permissions.
/// Refuses empty material with [`ScmApiError::AuthorizationNotIssued`], tampered or
/// invalid tokens with [`ScmApiError::AccessDenied`], wildcard, punctuation, or drifted service
/// names with [`ScmApiError::NameDrift`], image token mismatches with
/// [`ScmApiError::IdentityMismatch`], execution identity mismatches with
/// [`ScmApiError::IdentityMismatch`], target host token mismatches with
/// [`ScmApiError::AccessDenied`], candidate commit mismatches with
/// [`ScmApiError::AccessDenied`], transaction id mismatches with
/// [`ScmApiError::AccessDenied`], excessive permissions with
/// [`ScmApiError::AccessDenied`], expired or not-yet-valid validity windows with
/// [`ScmApiError::AccessDenied`], and replayed tokens/nonces across the shared
/// ledger with [`ScmApiError::AccessDenied`].
#[derive(Clone)]
pub struct TokenValidatingOwnerVerifier {
    authorized_scope: OwnerAuthorizationScope,
    authority_secret: Option<Vec<u8>>,
    consumed_nonces: Arc<Mutex<std::collections::BTreeSet<String>>>,
    ledger: SharedAuthorizationLedger,
    clock: Arc<dyn TrustedClock>,
}

impl std::fmt::Debug for TokenValidatingOwnerVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenValidatingOwnerVerifier")
            .field("authorized_scope", &self.authorized_scope)
            .field("has_authority_secret", &self.authority_secret.is_some())
            .finish()
    }
}

impl TokenValidatingOwnerVerifier {
    /// Constructs verifier for an expected token hash and bound scope.
    ///
    /// In this build, external callers receive an unissued verifier (no authority secret).
    /// Calling `verify()` unconditionally returns `Err(ScmApiError::AuthorizationNotIssued)`.
    pub fn new(expected_token: &[u8], scope: OwnerAuthorizationScope) -> Self {
        Self::with_clock_and_ledger(
            expected_token,
            scope,
            None,
            Arc::new(SystemClock),
            SharedAuthorizationLedger::global().clone(),
        )
    }

    /// Internal test constructor bound to an authority secret.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn from_authority(
        authority_secret: impl Into<Vec<u8>>,
        scope: OwnerAuthorizationScope,
    ) -> Self {
        let secret = authority_secret.into();
        Self::with_clock_and_ledger(
            &[],
            scope,
            Some(secret),
            Arc::new(SystemClock),
            SharedAuthorizationLedger::global().clone(),
        )
    }

    #[cfg(test)]
    pub(crate) fn with_authority_and_clock(
        authority_secret: impl Into<Vec<u8>>,
        scope: OwnerAuthorizationScope,
        clock: Arc<dyn TrustedClock>,
    ) -> Self {
        Self::with_clock_and_ledger(
            &[],
            scope,
            Some(authority_secret.into()),
            clock,
            SharedAuthorizationLedger::global().clone(),
        )
    }

    /// Internal/test constructor with a durable persistent ledger file.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_durable_ledger(
        authority_secret: impl Into<Vec<u8>>,
        scope: OwnerAuthorizationScope,
        clock: Arc<dyn TrustedClock>,
        ledger_path: impl Into<PathBuf>,
    ) -> io::Result<Self> {
        let ledger = SharedAuthorizationLedger::with_durable_file(ledger_path)?;
        Ok(Self::with_clock_and_ledger(
            &[],
            scope,
            Some(authority_secret.into()),
            clock,
            ledger,
        ))
    }

    pub(crate) fn with_clock_and_ledger(
        _expected_token: &[u8],
        scope: OwnerAuthorizationScope,
        authority_secret: Option<Vec<u8>>,
        clock: Arc<dyn TrustedClock>,
        ledger: SharedAuthorizationLedger,
    ) -> Self {
        Self {
            authorized_scope: scope,
            authority_secret,
            consumed_nonces: Arc::new(Mutex::new(std::collections::BTreeSet::new())),
            ledger,
            clock,
        }
    }

    /// Number of distinct nonces consumed so far by this verifier.
    pub fn consumed_nonce_count(&self) -> usize {
        self.consumed_nonces
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }
}

impl TrustedOwnerAuthorizationVerifier for TokenValidatingOwnerVerifier {
    fn verify(
        &self,
        request: &TrustedOwnerAuthorizationRequest,
    ) -> Result<LiveScmExecutionGate, ScmApiError> {
        if request.owner_material.is_empty() {
            return Err(ScmApiError::AuthorizationNotIssued);
        }

        let mut hasher = Sha256::new();
        hasher.update(&request.owner_material);
        let given_hash: [u8; 32] = hasher.finalize().into();

        // 1. Service name verification
        if request.service_name.trim().is_empty()
            || request.service_name.contains('*')
            || request.service_name.contains('?')
            || request.service_name.contains('/')
            || request.service_name.contains('\\')
            || !request
                .service_name
                .eq_ignore_ascii_case(&self.authorized_scope.service_name)
        {
            return Err(ScmApiError::NameDrift);
        }

        // 2. Image token verification
        if request.image_token.trim().is_empty()
            || !request
                .image_token
                .eq_ignore_ascii_case(&self.authorized_scope.image_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }

        // 3. Execution identity token verification
        if request.execution_identity_token.trim().is_empty()
            || !request
                .execution_identity_token
                .eq_ignore_ascii_case(&self.authorized_scope.execution_identity_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }

        // 4. Target host token verification
        if request.target_host_token.trim().is_empty()
            || request.target_host_token != self.authorized_scope.target_host_token
        {
            return Err(ScmApiError::AccessDenied);
        }

        // 5. Candidate commit verification
        if request.candidate_commit.trim().is_empty()
            || !request
                .candidate_commit
                .eq_ignore_ascii_case(&self.authorized_scope.candidate_commit)
        {
            return Err(ScmApiError::AccessDenied);
        }

        // 6. Transaction ID verification
        if request.transaction_id.trim().is_empty()
            || request.transaction_id != self.authorized_scope.transaction_id
        {
            return Err(ScmApiError::AccessDenied);
        }

        // 7. Permissions check
        if (request.allow_start && !self.authorized_scope.allow_start)
            || (request.allow_stop && !self.authorized_scope.allow_stop)
            || (request.allow_change_config && !self.authorized_scope.allow_change_config)
        {
            return Err(ScmApiError::AccessDenied);
        }

        // 8. Nonce check
        if request.nonce.trim().is_empty() {
            return Err(ScmApiError::AccessDenied);
        }

        // 9. Validity window check
        if request.expires_at_epoch_seconds <= request.not_before_epoch_seconds {
            return Err(ScmApiError::AccessDenied);
        }

        // 10. Trusted boundary clock check
        let now = self.clock.now_epoch_seconds();
        if now < request.not_before_epoch_seconds || now >= request.expires_at_epoch_seconds {
            return Err(ScmApiError::AccessDenied);
        }

        // 11. Cryptographic HMAC-SHA256 voucher verification with constant-time check
        if let Some(ref authority_secret) = self.authority_secret {
            let mut mac = HmacSha256::new_from_slice(authority_secret)
                .map_err(|_| ScmApiError::AccessDenied)?;
            mac.update(b"RUNNERMESH_OWNER_AUTH_V1\0");
            mac.update(request.service_name.as_bytes());
            mac.update(b"\0");
            mac.update(request.image_token.as_bytes());
            mac.update(b"\0");
            mac.update(request.execution_identity_token.as_bytes());
            mac.update(b"\0");
            mac.update(request.target_host_token.as_bytes());
            mac.update(b"\0");
            mac.update(request.candidate_commit.as_bytes());
            mac.update(b"\0");
            mac.update(&[
                request.allow_start as u8,
                request.allow_stop as u8,
                request.allow_change_config as u8,
            ]);
            mac.update(b"\0");
            mac.update(request.transaction_id.as_bytes());
            mac.update(b"\0");
            mac.update(&request.not_before_epoch_seconds.to_le_bytes());
            mac.update(&request.expires_at_epoch_seconds.to_le_bytes());
            mac.update(request.nonce.as_bytes());

            // Constant-time verification interface provided by digest/Mac trait
            if mac.verify_slice(&request.owner_material).is_err() {
                return Err(ScmApiError::AccessDenied);
            }
        } else {
            return Err(ScmApiError::AuthorizationNotIssued);
        }

        // Atomic single-use consumption across shared ledger to eliminate race conditions
        if !self.ledger.try_consume(given_hash, &request.nonce) {
            return Err(ScmApiError::AccessDenied);
        }
        self.consumed_nonces
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(request.nonce.clone());

        LiveScmExecutionGate::mint_trusted_live(
            &request.service_name,
            &request.image_token,
            &request.execution_identity_token,
            &request.target_host_token,
            &request.candidate_commit,
            &request.transaction_id,
            request.allow_start,
            request.allow_stop,
            request.allow_change_config,
            request.not_before_epoch_seconds,
            request.expires_at_epoch_seconds,
        )
    }
}

/// Explicitly forgeable test grant. Not Owner authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntheticScmTestGrant {
    service_name: String,
    image_token: String,
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
}

impl SyntheticScmTestGrant {
    pub fn new(
        service_name: impl Into<String>,
        image_token: impl Into<String>,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            image_token: image_token.into(),
            allow_start,
            allow_stop,
            allow_change_config,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScmGrantKind {
    Closed,
    /// Forgeable; valid only for mock/synthetic qualification.
    SyntheticTestOnly,
    /// Reserved for future trusted verifier issuance (not constructed this Train).
    #[allow(dead_code)]
    TrustedLive,
}

/// Hard gate for SCM mutation. Public bool construction is impossible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveScmExecutionGate {
    allow_start: bool,
    allow_stop: bool,
    allow_change_config: bool,
    grant_kind: ScmGrantKind,
    bound_service_name: String,
    bound_image_token: String,
    bound_execution_identity_token: String,
    bound_target_host_token: String,
    bound_candidate_commit: String,
    bound_transaction_id: String,
    not_before_epoch_seconds: u64,
    expires_at_epoch_seconds: u64,
}

impl Default for LiveScmExecutionGate {
    fn default() -> Self {
        Self::CLOSED
    }
}

/// Execution context presented at dispatch time to be verified against the gate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScmExecutionContext<'a> {
    pub service_name: &'a str,
    pub image_token: &'a str,
    pub execution_identity_token: &'a str,
    pub target_host_token: &'a str,
    pub candidate_commit: &'a str,
    pub transaction_id: &'a str,
    pub need_start: bool,
    pub need_stop: bool,
    pub need_config: bool,
}

impl LiveScmExecutionGate {
    pub const CLOSED: Self = Self {
        allow_start: false,
        allow_stop: false,
        allow_change_config: false,
        grant_kind: ScmGrantKind::Closed,
        bound_service_name: String::new(),
        bound_image_token: String::new(),
        bound_execution_identity_token: String::new(),
        bound_target_host_token: String::new(),
        bound_candidate_commit: String::new(),
        bound_transaction_id: String::new(),
        not_before_epoch_seconds: 0,
        expires_at_epoch_seconds: 0,
    };

    /// Opens a **synthetic test** gate. Never use for live Windows mutation.
    pub fn from_synthetic_test_grant(grant: &SyntheticScmTestGrant) -> Result<Self, ScmApiError> {
        if grant.service_name.trim().is_empty() || grant.service_name.contains('*') {
            return Err(ScmApiError::NameDrift);
        }
        Ok(Self {
            allow_start: grant.allow_start,
            allow_stop: grant.allow_stop,
            allow_change_config: grant.allow_change_config,
            grant_kind: ScmGrantKind::SyntheticTestOnly,
            bound_service_name: grant.service_name.clone(),
            bound_image_token: grant.image_token.clone(),
            bound_execution_identity_token: String::new(),
            bound_target_host_token: String::new(),
            bound_candidate_commit: String::new(),
            bound_transaction_id: String::new(),
            not_before_epoch_seconds: 0,
            expires_at_epoch_seconds: u64::MAX,
        })
    }

    /// Live path: delegates to a trusted verifier (unissued this Train).
    pub fn from_trusted_owner_authorization<V: TrustedOwnerAuthorizationVerifier>(
        verifier: &V,
        request: &TrustedOwnerAuthorizationRequest,
    ) -> Result<Self, ScmApiError> {
        verifier.verify(request)
    }

    /// Mints a trusted live gate from verified Owner authorization.
    ///
    /// Requires non-empty exact `service_name` and `image_token` (no wildcards).
    /// Public boolean construction is prevented: this is only callable by a validated
    /// [`TrustedOwnerAuthorizationVerifier`]. Not accessible outside this crate.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mint_trusted_live(
        service_name: impl Into<String>,
        image_token: impl Into<String>,
        execution_identity_token: impl Into<String>,
        target_host_token: impl Into<String>,
        candidate_commit: impl Into<String>,
        transaction_id: impl Into<String>,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
        not_before_epoch_seconds: u64,
        expires_at_epoch_seconds: u64,
    ) -> Result<Self, ScmApiError> {
        let s = service_name.into();
        let i = image_token.into();
        let e = execution_identity_token.into();
        let h = target_host_token.into();
        let c = candidate_commit.into();
        let t = transaction_id.into();
        if s.trim().is_empty() || s.contains('*') || s.contains('?') {
            return Err(ScmApiError::NameDrift);
        }
        if i.trim().is_empty()
            || e.trim().is_empty()
            || h.trim().is_empty()
            || c.trim().is_empty()
            || t.trim().is_empty()
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        if expires_at_epoch_seconds <= not_before_epoch_seconds {
            return Err(ScmApiError::AccessDenied);
        }
        Ok(Self {
            allow_start,
            allow_stop,
            allow_change_config,
            grant_kind: ScmGrantKind::TrustedLive,
            bound_service_name: s,
            bound_image_token: i,
            bound_execution_identity_token: e,
            bound_target_host_token: h,
            bound_candidate_commit: c,
            bound_transaction_id: t,
            not_before_epoch_seconds,
            expires_at_epoch_seconds,
        })
    }

    pub fn allows_mutation(&self) -> bool {
        !matches!(self.grant_kind, ScmGrantKind::Closed)
            && (self.allow_start || self.allow_stop || self.allow_change_config)
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.grant_kind, ScmGrantKind::Closed)
    }

    pub fn is_synthetic_test_only(&self) -> bool {
        matches!(self.grant_kind, ScmGrantKind::SyntheticTestOnly)
    }

    pub fn is_trusted_live(&self) -> bool {
        matches!(self.grant_kind, ScmGrantKind::TrustedLive)
    }

    pub fn bound_service_name(&self) -> &str {
        &self.bound_service_name
    }

    pub fn bound_image_token(&self) -> &str {
        &self.bound_image_token
    }

    pub fn bound_execution_identity_token(&self) -> &str {
        &self.bound_execution_identity_token
    }

    pub fn bound_target_host_token(&self) -> &str {
        &self.bound_target_host_token
    }

    pub fn bound_candidate_commit(&self) -> &str {
        &self.bound_candidate_commit
    }

    pub fn bound_transaction_id(&self) -> &str {
        &self.bound_transaction_id
    }

    pub fn not_before_epoch_seconds(&self) -> u64 {
        self.not_before_epoch_seconds
    }

    pub fn expires_at_epoch_seconds(&self) -> u64 {
        self.expires_at_epoch_seconds
    }

    pub fn authorizes_context(
        &self,
        context: &ScmExecutionContext<'_>,
        clock: &dyn TrustedClock,
    ) -> Result<(), ScmApiError> {
        if matches!(self.grant_kind, ScmGrantKind::Closed) {
            return Err(ScmApiError::GateClosed);
        }

        // Action permission check
        if context.need_start && !self.allow_start {
            return Err(ScmApiError::GateClosed);
        }
        if context.need_stop && !self.allow_stop {
            return Err(ScmApiError::GateClosed);
        }
        if context.need_config && !self.allow_change_config {
            return Err(ScmApiError::GateClosed);
        }

        // Service name check (must match, case-insensitive)
        if !self.bound_service_name.is_empty()
            && !self
                .bound_service_name
                .eq_ignore_ascii_case(context.service_name)
        {
            return Err(ScmApiError::NameDrift);
        }

        // Image token check
        if !self.bound_image_token.is_empty()
            && !self
                .bound_image_token
                .eq_ignore_ascii_case(context.image_token)
        {
            return Err(ScmApiError::GateClosed);
        }

        // For TrustedLive, enforce all remaining P03 dimensions strictly against runtime context and clock
        if matches!(self.grant_kind, ScmGrantKind::TrustedLive) {
            // Execution identity check
            if !self.bound_execution_identity_token.is_empty()
                && !self
                    .bound_execution_identity_token
                    .eq_ignore_ascii_case(context.execution_identity_token)
            {
                return Err(ScmApiError::IdentityMismatch);
            }

            // Target host check
            if !self.bound_target_host_token.is_empty()
                && !self
                    .bound_target_host_token
                    .eq_ignore_ascii_case(context.target_host_token)
            {
                return Err(ScmApiError::IdentityMismatch);
            }

            // Candidate commit check
            if !self.bound_candidate_commit.is_empty()
                && !self
                    .bound_candidate_commit
                    .eq_ignore_ascii_case(context.candidate_commit)
            {
                return Err(ScmApiError::IdentityMismatch);
            }

            // Transaction ID check
            if context.transaction_id.trim().is_empty() {
                return Err(ScmApiError::RestoreIncomplete);
            }
            if !self.bound_transaction_id.is_empty()
                && self.bound_transaction_id != context.transaction_id
            {
                return Err(ScmApiError::IdentityMismatch);
            }

            // Trusted clock & validity window check (post-mint expiry)
            let now = clock.now_epoch_seconds();
            if now < self.not_before_epoch_seconds || now >= self.expires_at_epoch_seconds {
                return Err(ScmApiError::AccessDenied);
            }
        } else if matches!(self.grant_kind, ScmGrantKind::SyntheticTestOnly) {
            if !self.bound_execution_identity_token.is_empty()
                && !context.execution_identity_token.is_empty()
                && !self
                    .bound_execution_identity_token
                    .eq_ignore_ascii_case(context.execution_identity_token)
            {
                return Err(ScmApiError::IdentityMismatch);
            }
            if !self.bound_target_host_token.is_empty()
                && !context.target_host_token.is_empty()
                && !self
                    .bound_target_host_token
                    .eq_ignore_ascii_case(context.target_host_token)
            {
                return Err(ScmApiError::IdentityMismatch);
            }
            if !self.bound_candidate_commit.is_empty()
                && !context.candidate_commit.is_empty()
                && !self
                    .bound_candidate_commit
                    .eq_ignore_ascii_case(context.candidate_commit)
            {
                return Err(ScmApiError::IdentityMismatch);
            }
            if !self.bound_transaction_id.is_empty()
                && !context.transaction_id.is_empty()
                && self.bound_transaction_id != context.transaction_id
            {
                return Err(ScmApiError::IdentityMismatch);
            }
            if self.expires_at_epoch_seconds > 0 && self.expires_at_epoch_seconds < u64::MAX {
                let now = clock.now_epoch_seconds();
                if now >= self.expires_at_epoch_seconds {
                    return Err(ScmApiError::AccessDenied);
                }
            }
        }

        Ok(())
    }

    pub fn authorizes(
        &self,
        service_name: &str,
        image_token: &str,
        need_start: bool,
        need_stop: bool,
        need_config: bool,
    ) -> bool {
        if matches!(self.grant_kind, ScmGrantKind::Closed) {
            return false;
        }
        if !self.bound_service_name.is_empty()
            && !self.bound_service_name.eq_ignore_ascii_case(service_name)
        {
            return false;
        }
        if !self.bound_image_token.is_empty()
            && !self.bound_image_token.eq_ignore_ascii_case(image_token)
        {
            return false;
        }
        if need_start && !self.allow_start {
            return false;
        }
        if need_stop && !self.allow_stop {
            return false;
        }
        if need_config && !self.allow_change_config {
            return false;
        }
        true
    }
}

/// Durable transaction stages for crash recovery analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScmTransactionStage {
    CapturedPrestate,
    WithdrawingAdmission,
    WaitingIdle,
    RequestedStop,
    StopReadback,
    RequestedStart,
    StartReadback,
    Restoring,
    Completed,
    Refused,
    Interrupted,
    RestoreFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScmTransactionReceipt {
    pub service_name: String,
    pub stage: ScmTransactionStage,
    pub api_accepted: bool,
    pub achieved: AchievedServiceControl,
    pub reason: Option<String>,
    pub restore: Option<ServiceRestorePlan>,
    /// Independent restore readback verdict. Never true unless every restore
    /// step succeeded and final observation matched the bound plan.
    pub restore_pass: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScmApiError {
    AccessDenied,
    Timeout,
    NotFound,
    NameDrift,
    Unavailable,
    GateClosed,
    BusyWorker,
    IdentityMismatch,
    StateUnknown,
    RestoreIncomplete,
    AuthorizationNotIssued,
    AdmissionObserveFailed,
}

impl ScmApiError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccessDenied => "scm.access-denied",
            Self::Timeout => "scm.timeout",
            Self::NotFound => "scm.not-found",
            Self::NameDrift => "scm.name-drift",
            Self::Unavailable => "scm.unavailable",
            Self::GateClosed => "scm.live-gate-closed",
            Self::BusyWorker => "scm.busy-worker",
            Self::IdentityMismatch => "scm.identity-mismatch",
            Self::StateUnknown => "scm.state-unknown",
            Self::RestoreIncomplete => "scm.restore-incomplete",
            Self::AuthorizationNotIssued => "scm.authorization-not-issued",
            Self::AdmissionObserveFailed => "scm.admission-observe-failed",
        }
    }
}

/// Exact host snapshot used for identity binding. Image tokens are opaque.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScmExactSnapshot {
    pub observation: ServiceObservation,
    pub image_token: String,
    pub execution_identity_token: String,
}

/// Injectable SCM transport. Production Windows adapter must honor the live gate.
pub trait ScmControlBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError>;
    fn start(&self, service_name: &str) -> Result<(), ScmApiError>;
    fn stop(&self, service_name: &str) -> Result<(), ScmApiError>;
    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError>;
}

/// In-memory SCM used by synthetic HUX-2 and unit tests.
#[derive(Clone, Default)]
pub struct MockScmBackend {
    inner: Arc<Mutex<MockScmState>>,
}

#[derive(Clone, Debug, Default)]
struct MockScmState {
    services: BTreeMap<String, MockServiceRecord>,
    fail_next: Option<ScmApiError>,
    /// Remaining successful `query_exact` calls before the next query fails.
    fail_after_queries: Option<(u32, ScmApiError)>,
    fail_change: Option<ScmApiError>,
    stop_calls: u32,
    start_calls: u32,
    change_calls: u32,
    service_stop_calls: BTreeMap<String, u32>,
    service_start_calls: BTreeMap<String, u32>,
    service_actions: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct MockServiceRecord {
    pub observation: ServiceObservation,
    pub worker_busy: bool,
    pub image_token: String,
    pub execution_identity_token: String,
}

impl MockScmBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, record: MockServiceRecord) {
        let key = record.observation.service_name.to_ascii_lowercase();
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .services
            .insert(key, record);
    }

    pub fn set_fail_next(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next = Some(error);
    }

    /// Allow `successful_before_fail` more successful queries, then fail once.
    pub fn set_fail_after_queries(&self, successful_before_fail: u32, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_after_queries = Some((successful_before_fail, error));
    }

    pub fn set_fail_change(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_change = Some(error);
    }

    pub fn set_worker_busy(&self, service_name: &str, busy: bool) {
        let key = service_name.to_ascii_lowercase();
        if let Some(record) = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .services
            .get_mut(&key)
        {
            record.worker_busy = busy;
            if busy {
                record.observation.state = DiscoveredLifecycle::Busy;
            } else if matches!(record.observation.state, DiscoveredLifecycle::Busy) {
                record.observation.state = DiscoveredLifecycle::Listening;
            }
        }
    }

    pub fn stop_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stop_calls
    }

    pub fn start_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .start_calls
    }

    pub fn change_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .change_calls
    }

    pub fn service_stop_calls(&self, service_name: &str) -> u32 {
        let key = service_name.to_ascii_lowercase();
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .service_stop_calls
            .get(&key)
            .copied()
            .unwrap_or(0)
    }

    pub fn service_start_calls(&self, service_name: &str) -> u32 {
        let key = service_name.to_ascii_lowercase();
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .service_start_calls
            .get(&key)
            .copied()
            .unwrap_or(0)
    }

    pub fn service_actions(&self, service_name: &str) -> Vec<String> {
        let key = service_name.to_ascii_lowercase();
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .service_actions
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    fn take_fail(&self) -> Result<(), ScmApiError> {
        if let Some(error) = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next
            .take()
        {
            return Err(error);
        }
        Ok(())
    }
}

impl ScmControlBackend for MockScmBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((remaining, error)) = guard.fail_after_queries {
            if remaining == 0 {
                guard.fail_after_queries = None;
                return Err(error);
            }
            guard.fail_after_queries = Some((remaining.saturating_sub(1), error));
        }
        let record = guard
            .services
            .get(&key)
            .cloned()
            .ok_or(ScmApiError::NotFound)?;
        let mut observation = record.observation;
        if record.worker_busy {
            observation.state = DiscoveredLifecycle::Busy;
        }
        Ok(ScmExactSnapshot {
            observation,
            image_token: record.image_token,
            execution_identity_token: record.execution_identity_token,
        })
    }

    fn start(&self, service_name: &str) -> Result<(), ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        {
            let record = guard.services.get_mut(&key).ok_or(ScmApiError::NotFound)?;
            record.observation.state = DiscoveredLifecycle::Listening;
        }
        guard.start_calls = guard.start_calls.saturating_add(1);
        *guard.service_start_calls.entry(key.clone()).or_insert(0) += 1;
        guard
            .service_actions
            .entry(key)
            .or_default()
            .push("start".to_string());
        Ok(())
    }

    fn stop(&self, service_name: &str) -> Result<(), ScmApiError> {
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        {
            let record = guard.services.get_mut(&key).ok_or(ScmApiError::NotFound)?;
            if record.worker_busy {
                return Err(ScmApiError::BusyWorker);
            }
            record.observation.state = DiscoveredLifecycle::Stopped;
        }
        guard.stop_calls = guard.stop_calls.saturating_add(1);
        *guard.service_stop_calls.entry(key.clone()).or_insert(0) += 1;
        guard
            .service_actions
            .entry(key)
            .or_default()
            .push("stop".to_string());
        Ok(())
    }

    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError> {
        if let Some(error) = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_change
            .take()
        {
            return Err(error);
        }
        self.take_fail()?;
        let key = service_name.to_ascii_lowercase();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        {
            let record = guard.services.get_mut(&key).ok_or(ScmApiError::NotFound)?;
            record.observation.start_mode = mode;
        }
        guard.change_calls = guard.change_calls.saturating_add(1);
        guard
            .service_actions
            .entry(key)
            .or_default()
            .push(format!("change_start_mode:{:?}", mode));
        Ok(())
    }
}

/// Fake GitHub admission backend for synthetic HUX-2 (never talks to GitHub).
#[derive(Clone, Default)]
pub struct FakeGithubAdmissionBackend {
    inner: Arc<Mutex<FakeGithubState>>,
}

#[derive(Clone, Debug)]
struct FakeGithubState {
    selector_present: bool,
    /// Desired presence after pending eventual-visibility lag expires.
    pending_selector_present: Option<bool>,
    /// Observe calls remaining before `pending_selector_present` becomes visible.
    eventual_visibility_lag: u32,
    fail_next_observe: Option<ScmApiError>,
    fail_next_withdraw: Option<ScmApiError>,
    fail_next_restore: Option<ScmApiError>,
    disconnected: bool,
    stale_token: bool,
    /// Opaque registration-scope token expected by the Agent binding.
    expected_scope_token: Option<String>,
    /// Opaque registration-scope token last observed from the fake API surface.
    observed_scope_token: Option<String>,
    withdraw_calls: u32,
    restore_calls: u32,
    observe_calls: u32,
}

impl Default for FakeGithubState {
    fn default() -> Self {
        Self {
            selector_present: true,
            pending_selector_present: None,
            eventual_visibility_lag: 0,
            fail_next_observe: None,
            fail_next_withdraw: None,
            fail_next_restore: None,
            disconnected: false,
            stale_token: false,
            expected_scope_token: None,
            observed_scope_token: None,
            withdraw_calls: 0,
            restore_calls: 0,
            observe_calls: 0,
        }
    }
}

impl FakeGithubAdmissionBackend {
    pub fn new_with_selector_present(present: bool) -> Self {
        let backend = Self::default();
        backend
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .selector_present = present;
        backend
    }

    pub fn set_fail_next_observe(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next_observe = Some(error);
    }

    pub fn set_fail_next_withdraw(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next_withdraw = Some(error);
    }

    pub fn set_fail_next_restore(&self, error: ScmApiError) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .fail_next_restore = Some(error);
    }

    pub fn set_disconnected(&self, disconnected: bool) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .disconnected = disconnected;
    }

    /// Models a stale / expired GitHub token: all mutations and observations fail closed.
    pub fn set_stale_token(&self, stale: bool) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stale_token = stale;
    }

    /// After a successful withdraw/restore, keep the previous observed value for
    /// `lag` observe calls before the new presence becomes visible.
    pub fn set_eventual_visibility_lag(&self, lag: u32) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .eventual_visibility_lag = lag;
    }

    /// Binds FakeGithub observations/mutations to an opaque registration scope.
    /// When `observed` diverges from `expected`, the backend fails closed with
    /// `IdentityMismatch` and does not rewrite selector presence.
    pub fn set_scope_tokens(&self, expected: impl Into<String>, observed: impl Into<String>) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.expected_scope_token = Some(expected.into());
        guard.observed_scope_token = Some(observed.into());
    }

    fn refuse_scope_drift(guard: &FakeGithubState) -> Result<(), ScmApiError> {
        match (
            guard.expected_scope_token.as_deref(),
            guard.observed_scope_token.as_deref(),
        ) {
            (Some(expected), Some(observed)) if expected != observed => {
                Err(ScmApiError::IdentityMismatch)
            }
            _ => Ok(()),
        }
    }

    pub fn withdraw_capacity(&self) -> Result<(), ScmApiError> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if guard.stale_token {
            return Err(ScmApiError::AccessDenied);
        }
        if guard.disconnected {
            return Err(ScmApiError::Unavailable);
        }
        Self::refuse_scope_drift(&guard)?;
        if let Some(error) = guard.fail_next_withdraw.take() {
            return Err(error);
        }
        Self::queue_presence(&mut guard, false);
        guard.withdraw_calls = guard.withdraw_calls.saturating_add(1);
        Ok(())
    }

    pub fn restore_capacity(&self) -> Result<(), ScmApiError> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if guard.stale_token {
            return Err(ScmApiError::AccessDenied);
        }
        if guard.disconnected {
            return Err(ScmApiError::Unavailable);
        }
        Self::refuse_scope_drift(&guard)?;
        if let Some(error) = guard.fail_next_restore.take() {
            return Err(error);
        }
        Self::queue_presence(&mut guard, true);
        guard.restore_calls = guard.restore_calls.saturating_add(1);
        Ok(())
    }

    fn queue_presence(guard: &mut FakeGithubState, present: bool) {
        if guard.eventual_visibility_lag == 0 {
            guard.selector_present = present;
            guard.pending_selector_present = None;
            return;
        }
        guard.pending_selector_present = Some(present);
    }

    fn advance_eventual_visibility(guard: &mut FakeGithubState) {
        let Some(pending) = guard.pending_selector_present else {
            return;
        };
        // Consume one lag slot per observe while keeping the previous value.
        if guard.eventual_visibility_lag > 0 {
            guard.eventual_visibility_lag = guard.eventual_visibility_lag.saturating_sub(1);
            return;
        }
        guard.selector_present = pending;
        guard.pending_selector_present = None;
    }

    pub fn withdraw_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .withdraw_calls
    }

    pub fn restore_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .restore_calls
    }

    pub fn observe_calls(&self) -> u32 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .observe_calls
    }
}

impl AdmissionSelectorObserver for FakeGithubAdmissionBackend {
    fn observe_selector_absent(&mut self) -> Result<bool, ScmApiError> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.observe_calls = guard.observe_calls.saturating_add(1);
        if guard.stale_token {
            return Err(ScmApiError::AccessDenied);
        }
        if let Some(error) = guard.fail_next_observe.take() {
            return Err(error);
        }
        if guard.disconnected {
            return Err(ScmApiError::Unavailable);
        }
        Self::refuse_scope_drift(&guard)?;
        Self::advance_eventual_visibility(&mut guard);
        Ok(!guard.selector_present)
    }
}

impl crate::AdmissionControlBackend for FakeGithubAdmissionBackend {
    fn observe_admission_selector(
        &mut self,
    ) -> Result<crate::RemoteAdmissionObservation, crate::AdmissionBackendError> {
        let absent = self
            .observe_selector_absent()
            .map_err(|_| crate::AdmissionBackendError::ApiUnavailable)?;
        Ok(crate::RemoteAdmissionObservation {
            selector: if absent {
                crate::AdmissionSelectorState::Absent
            } else {
                crate::AdmissionSelectorState::Present
            },
            runner_busy: false,
            runner_online: true,
        })
    }

    fn advertise_capacity(
        &mut self,
    ) -> Result<crate::RemoteAdmissionObservation, crate::AdmissionBackendError> {
        FakeGithubAdmissionBackend::restore_capacity(self)
            .map_err(|_| crate::AdmissionBackendError::ApiUnavailable)?;
        Ok(crate::RemoteAdmissionObservation {
            selector: crate::AdmissionSelectorState::Present,
            runner_busy: false,
            runner_online: true,
        })
    }

    fn withdraw_capacity(
        &mut self,
    ) -> Result<crate::RemoteAdmissionObservation, crate::AdmissionBackendError> {
        FakeGithubAdmissionBackend::withdraw_capacity(self)
            .map_err(|_| crate::AdmissionBackendError::ApiUnavailable)?;
        Ok(crate::RemoteAdmissionObservation {
            selector: crate::AdmissionSelectorState::Absent,
            runner_busy: false,
            runner_online: true,
        })
    }
}

/// Exact-identity bound executor. Live mutation requires a trusted Owner gate.
pub struct ScmControlExecutor<B> {
    backend: B,
    gate: LiveScmExecutionGate,
    expected_service_name: String,
    expected_image_token: String,
    expected_execution_identity_token: String,
    expected_target_host_token: String,
    expected_candidate_commit: String,
    clock: Arc<dyn TrustedClock>,
}

impl<B: ScmControlBackend> ScmControlExecutor<B> {
    pub fn new(
        backend: B,
        expected_service_name: impl Into<String>,
        expected_image_token: impl Into<String>,
        expected_execution_identity_token: impl Into<String>,
        gate: LiveScmExecutionGate,
    ) -> Self {
        let expected_target_host_token = gate.bound_target_host_token.clone();
        let expected_candidate_commit = gate.bound_candidate_commit.clone();
        Self {
            backend,
            expected_service_name: expected_service_name.into(),
            expected_image_token: expected_image_token.into(),
            expected_execution_identity_token: expected_execution_identity_token.into(),
            expected_target_host_token,
            expected_candidate_commit,
            gate,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn TrustedClock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn with_host_and_commit(
        mut self,
        target_host_token: impl Into<String>,
        candidate_commit: impl Into<String>,
    ) -> Self {
        self.expected_target_host_token = target_host_token.into();
        self.expected_candidate_commit = candidate_commit.into();
        self
    }

    pub fn capture_prestate(
        &self,
        worker_busy: bool,
        admission_selector_absent: bool,
    ) -> Result<ServicePrestateCapture, ScmApiError> {
        let snap = self.query_bound_snapshot()?;
        let observed = &snap.observation;
        let transaction_id = if !self.gate.bound_transaction_id.is_empty() {
            self.gate.bound_transaction_id.clone()
        } else {
            new_transaction_id()
        };
        Ok(ServicePrestateCapture {
            service_name: observed.service_name.clone(),
            state: observed.state,
            start_mode: observed.start_mode,
            image_path_matches: observed.image_path_matches,
            ownership_verified: observed.ownership_verified,
            worker_busy,
            admission_selector_absent,
            image_token: snap.image_token,
            execution_identity_token: snap.execution_identity_token,
            transaction_id,
        })
    }

    /// Returns the backend observation **without** rewriting trust flags.
    pub fn observe_verified(&self) -> Result<ServiceObservation, ScmApiError> {
        Ok(self.query_bound_snapshot()?.observation)
    }

    fn query_bound_snapshot(&self) -> Result<ScmExactSnapshot, ScmApiError> {
        if self.expected_service_name.contains('*') || self.expected_service_name.trim().is_empty()
        {
            return Err(ScmApiError::NameDrift);
        }
        let snap = self.backend.query_exact(&self.expected_service_name)?;
        if !snap
            .observation
            .service_name
            .eq_ignore_ascii_case(&self.expected_service_name)
        {
            return Err(ScmApiError::NameDrift);
        }
        if !snap.observation.ownership_verified {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !snap.observation.image_path_matches {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !snap
            .image_token
            .eq_ignore_ascii_case(&self.expected_image_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !snap
            .execution_identity_token
            .eq_ignore_ascii_case(&self.expected_execution_identity_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        Ok(snap)
    }

    fn plan_matches_binding(&self, plan: &ServiceRestorePlan) -> Result<(), ScmApiError> {
        if !plan
            .service_name
            .eq_ignore_ascii_case(&self.expected_service_name)
        {
            return Err(ScmApiError::NameDrift);
        }
        if !plan
            .image_token
            .eq_ignore_ascii_case(&self.expected_image_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        if !plan
            .execution_identity_token
            .eq_ignore_ascii_case(&self.expected_execution_identity_token)
        {
            return Err(ScmApiError::IdentityMismatch);
        }
        if plan.transaction_id.trim().is_empty() {
            return Err(ScmApiError::RestoreIncomplete);
        }
        Ok(())
    }

    /// Advances one prepare/execute step.
    pub fn advance(
        &self,
        desired: DesiredServiceLifecycle,
        admission_selector_absent: bool,
        worker_busy: bool,
        managed_admission: bool,
        prestate: &ServicePrestateCapture,
    ) -> ScmTransactionReceipt {
        let restore = Some(capture_restore_plan(prestate));
        if let Err(error) = self.plan_matches_binding(restore.as_ref().unwrap()) {
            return refused_receipt(&self.expected_service_name, error, restore);
        }
        let observed = match self.observe_verified() {
            Ok(observed) => observed,
            Err(error) => {
                return refused_receipt(&self.expected_service_name, error, restore);
            }
        };
        let phase = next_control_phase_with_admission(
            desired,
            &observed,
            admission_selector_absent,
            worker_busy,
            managed_admission,
        );
        match phase {
            ServiceControlPhase::WithdrawAdmission => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::WithdrawingAdmission,
                api_accepted: true,
                achieved: AchievedServiceControl::Draining,
                reason: None,
                restore,
                restore_pass: false,
            },
            ServiceControlPhase::WaitIdle { .. } => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::WaitingIdle,
                api_accepted: true,
                achieved: AchievedServiceControl::Draining,
                reason: None,
                restore,
                restore_pass: false,
            },
            ServiceControlPhase::PrepareStop => self.execute_stop(restore),
            ServiceControlPhase::PrepareStart => self.execute_start(restore),
            ServiceControlPhase::Observe => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Completed,
                api_accepted: true,
                achieved: achieved_from_observation(&observed, worker_busy),
                reason: None,
                restore,
                restore_pass: false,
            },
            ServiceControlPhase::Restore => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Restoring,
                api_accepted: true,
                achieved: AchievedServiceControl::Unknown,
                reason: None,
                restore,
                restore_pass: false,
            },
            ServiceControlPhase::Refused { reason } => ScmTransactionReceipt {
                service_name: self.expected_service_name.clone(),
                stage: ScmTransactionStage::Refused,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(reason),
                restore,
                restore_pass: false,
            },
        }
    }

    fn execute_stop(&self, restore: Option<ServiceRestorePlan>) -> ScmTransactionReceipt {
        let tx_id = restore
            .as_ref()
            .map(|r| r.transaction_id.as_str())
            .unwrap_or(self.gate.bound_transaction_id());
        let ctx = ScmExecutionContext {
            service_name: &self.expected_service_name,
            image_token: &self.expected_image_token,
            execution_identity_token: &self.expected_execution_identity_token,
            target_host_token: &self.expected_target_host_token,
            candidate_commit: &self.expected_candidate_commit,
            transaction_id: tx_id,
            need_start: false,
            need_stop: true,
            need_config: false,
        };
        if let Err(err) = self.gate.authorizes_context(&ctx, self.clock.as_ref()) {
            return refused_receipt(&self.expected_service_name, err, restore);
        }
        match self.backend.stop(&self.expected_service_name) {
            Ok(()) => match self.observe_verified() {
                Ok(after) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::StopReadback,
                    api_accepted: true,
                    achieved: achieved_from_observation(&after, false),
                    reason: None,
                    restore,
                    restore_pass: false,
                },
                Err(error) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::Interrupted,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                },
            },
            Err(error) => refused_receipt(&self.expected_service_name, error, restore),
        }
    }

    fn execute_start(&self, restore: Option<ServiceRestorePlan>) -> ScmTransactionReceipt {
        let tx_id = restore
            .as_ref()
            .map(|r| r.transaction_id.as_str())
            .unwrap_or(self.gate.bound_transaction_id());
        let ctx = ScmExecutionContext {
            service_name: &self.expected_service_name,
            image_token: &self.expected_image_token,
            execution_identity_token: &self.expected_execution_identity_token,
            target_host_token: &self.expected_target_host_token,
            candidate_commit: &self.expected_candidate_commit,
            transaction_id: tx_id,
            need_start: true,
            need_stop: false,
            need_config: false,
        };
        if let Err(err) = self.gate.authorizes_context(&ctx, self.clock.as_ref()) {
            return refused_receipt(&self.expected_service_name, err, restore);
        }
        match self.backend.start(&self.expected_service_name) {
            Ok(()) => match self.observe_verified() {
                Ok(after) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::StartReadback,
                    api_accepted: true,
                    achieved: achieved_from_observation(&after, false),
                    reason: None,
                    restore,
                    restore_pass: false,
                },
                Err(error) => ScmTransactionReceipt {
                    service_name: self.expected_service_name.clone(),
                    stage: ScmTransactionStage::Interrupted,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                },
            },
            Err(error) => refused_receipt(&self.expected_service_name, error, restore),
        }
    }

    /// Applies restore plan under a sealed gate with independent step readback.
    ///
    /// Admission result is obtained from [`AdmissionSelectorObserver`] at
    /// verification time — never from a caller-supplied boolean fact.
    pub fn restore<A: AdmissionSelectorObserver>(
        &self,
        plan: &ServiceRestorePlan,
        admission: &mut A,
    ) -> ScmTransactionReceipt {
        let restore = Some(plan.clone());
        if let Err(error) = self.plan_matches_binding(plan) {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }
        if self.gate.is_closed() {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(ScmApiError::GateClosed.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        let ctx = ScmExecutionContext {
            service_name: &plan.service_name,
            image_token: &plan.image_token,
            execution_identity_token: &plan.execution_identity_token,
            target_host_token: &self.expected_target_host_token,
            candidate_commit: &self.expected_candidate_commit,
            transaction_id: &plan.transaction_id,
            need_start: plan.restore_running,
            need_stop: !plan.restore_running,
            need_config: true,
        };

        if self.gate.is_trusted_live() || plan.requires_owner_gate {
            if let Err(err) = self.gate.authorizes_context(&ctx, self.clock.as_ref()) {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: false,
                    achieved: AchievedServiceControl::Refused,
                    reason: Some(err.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        }

        if self.gate.is_trusted_live() || plan.requires_owner_gate {
            if let Err(err) = self.gate.authorizes_context(&ctx, self.clock.as_ref()) {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: false,
                    achieved: AchievedServiceControl::Refused,
                    reason: Some(err.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        }

        if let Err(error) = self
            .backend
            .change_start_mode(&plan.service_name, plan.restore_start_mode)
        {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        let after_mode = match self.observe_verified() {
            Ok(obs) => obs,
            Err(error) => {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        };
        if after_mode.start_mode != plan.restore_start_mode {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: true,
                achieved: AchievedServiceControl::Unknown,
                reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        if self.gate.is_trusted_live() || plan.requires_owner_gate {
            if let Err(err) = self.gate.authorizes_context(&ctx, self.clock.as_ref()) {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: false,
                    achieved: AchievedServiceControl::Refused,
                    reason: Some(err.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        }

        let run_result = if plan.restore_running {
            self.backend.start(&plan.service_name)
        } else {
            self.backend.stop(&plan.service_name)
        };
        if let Err(error) = run_result {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: false,
                achieved: AchievedServiceControl::Refused,
                reason: Some(error.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        let after = match self.observe_verified() {
            Ok(obs) => obs,
            Err(error) => {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: true,
                    achieved: AchievedServiceControl::Unknown,
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        };
        let running_now = matches!(
            after.state,
            DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
        );
        if running_now != plan.restore_running {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: true,
                achieved: achieved_from_observation(&after, false),
                reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
                restore,
                restore_pass: false,
            };
        }

        let admission_absent = match admission.observe_selector_absent() {
            Ok(absent) => absent,
            Err(error) => {
                return ScmTransactionReceipt {
                    service_name: plan.service_name.clone(),
                    stage: ScmTransactionStage::RestoreFailed,
                    api_accepted: true,
                    achieved: achieved_from_observation(&after, false),
                    reason: Some(error.as_str().to_owned()),
                    restore,
                    restore_pass: false,
                };
            }
        };
        if admission_absent != plan.admission_selector_absent {
            return ScmTransactionReceipt {
                service_name: plan.service_name.clone(),
                stage: ScmTransactionStage::RestoreFailed,
                api_accepted: true,
                achieved: achieved_from_observation(&after, false),
                reason: Some("scm.admission-readback-mismatch".to_owned()),
                restore,
                restore_pass: false,
            };
        }

        ScmTransactionReceipt {
            service_name: plan.service_name.clone(),
            stage: ScmTransactionStage::Completed,
            api_accepted: true,
            achieved: achieved_from_observation(&after, false),
            reason: None,
            restore,
            restore_pass: true,
        }
    }
}

fn refused_receipt(
    service_name: &str,
    error: ScmApiError,
    restore: Option<ServiceRestorePlan>,
) -> ScmTransactionReceipt {
    ScmTransactionReceipt {
        service_name: service_name.to_owned(),
        stage: ScmTransactionStage::Refused,
        api_accepted: false,
        achieved: AchievedServiceControl::Refused,
        reason: Some(error.as_str().to_owned()),
        restore,
        restore_pass: false,
    }
}

fn new_transaction_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("scm-tx-{nanos}")
}

/// Opaque fingerprint helper for image paths (never store raw private paths).
pub fn opaque_image_token(image_path: &str) -> String {
    let clean = crate::windows_runner_discovery::service_image_path_token(image_path)
        .unwrap_or_else(|| image_path.trim_matches('"'));
    let digest = Sha256::digest(clean.trim().to_ascii_lowercase().as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Canonical resolution of the Windows runner service binary location.
///
/// Typical runner layout places the service binary at `<runner_home>\bin\RunnerService.exe`.
/// If `<runner_home>\RunnerService.exe` exists directly, it is used. Otherwise canonical
/// `<runner_home>\bin\RunnerService.exe` is returned.
pub fn runner_service_binary_path(runner_home: &std::path::Path) -> std::path::PathBuf {
    let bin_path = runner_home.join("bin").join("RunnerService.exe");
    if bin_path.exists() {
        return bin_path;
    }
    let direct_path = runner_home.join("RunnerService.exe");
    if direct_path.exists() {
        return direct_path;
    }
    bin_path
}

/// Derives the authoritative opaque image token for a runner home directory
/// using its canonical runner service binary path.
pub fn runner_service_image_token(runner_home: &std::path::Path) -> String {
    opaque_image_token(&runner_service_binary_path(runner_home).to_string_lossy())
}

/// Unconstructable mutation arm. Public API never produces a live arm this Train.
#[derive(Clone, Debug)]
pub struct ScmMutationArm {
    _private: (),
}

/// Windows SCM backend with real query source and sealed write paths.
#[derive(Clone, Debug)]
pub struct WindowsScmBackend {
    allow_read_only_query: bool,
    /// Always `None` via [`WindowsScmBackend::mutation_inert`]. Write APIs exist
    /// in source for review but refuse without an arm.
    mutation_arm: Option<ScmMutationArm>,
}

impl Default for WindowsScmBackend {
    fn default() -> Self {
        Self::mutation_inert()
    }
}

impl WindowsScmBackend {
    pub fn mutation_inert() -> Self {
        Self {
            allow_read_only_query: true,
            mutation_arm: None,
        }
    }

    /// Future Owner-transaction entry. Not callable without a privately minted arm.
    #[allow(dead_code)]
    fn with_mutation_arm(arm: ScmMutationArm) -> Self {
        Self {
            allow_read_only_query: true,
            mutation_arm: Some(arm),
        }
    }

    fn require_exact_name(service_name: &str) -> Result<(), ScmApiError> {
        if service_name.contains('*') || service_name.trim().is_empty() {
            return Err(ScmApiError::NameDrift);
        }
        Ok(())
    }
}

impl ScmControlBackend for WindowsScmBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
        if !self.allow_read_only_query {
            return Err(ScmApiError::GateClosed);
        }
        Self::require_exact_name(service_name)?;
        #[cfg(windows)]
        {
            windows_query_exact(service_name)
        }
        #[cfg(not(windows))]
        {
            let _ = service_name;
            Err(ScmApiError::Unavailable)
        }
    }

    fn start(&self, service_name: &str) -> Result<(), ScmApiError> {
        Self::require_exact_name(service_name)?;
        if self.mutation_arm.is_none() {
            return Err(ScmApiError::GateClosed);
        }
        #[cfg(windows)]
        {
            windows_start_service(service_name)
        }
        #[cfg(not(windows))]
        {
            let _ = service_name;
            Err(ScmApiError::Unavailable)
        }
    }

    fn stop(&self, service_name: &str) -> Result<(), ScmApiError> {
        Self::require_exact_name(service_name)?;
        if self.mutation_arm.is_none() {
            return Err(ScmApiError::GateClosed);
        }
        #[cfg(windows)]
        {
            windows_stop_service(service_name)
        }
        #[cfg(not(windows))]
        {
            let _ = service_name;
            Err(ScmApiError::Unavailable)
        }
    }

    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError> {
        Self::require_exact_name(service_name)?;
        if self.mutation_arm.is_none() {
            return Err(ScmApiError::GateClosed);
        }
        #[cfg(windows)]
        {
            windows_change_start_mode(service_name, mode)
        }
        #[cfg(not(windows))]
        {
            let _ = (service_name, mode);
            Err(ScmApiError::Unavailable)
        }
    }
}

#[cfg(windows)]
fn windows_query_exact(service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
    use std::mem::{align_of, size_of, MaybeUninit};
    use std::ptr;
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST};
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW,
        QueryServiceStatusEx, QUERY_SERVICE_CONFIGW, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO,
        SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_STATUS_PROCESS,
    };

    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(
            scm,
            wide.as_ptr(),
            SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        );
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }

        // SC_STATUS_PROCESS_INFO returns SERVICE_STATUS_PROCESS (fixed size).
        let mut status = MaybeUninit::<SERVICE_STATUS_PROCESS>::zeroed();
        let mut needed = 0u32;
        let status_ok = QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            status.as_mut_ptr().cast::<u8>(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        );
        if status_ok == 0 {
            CloseServiceHandle(service);
            CloseServiceHandle(scm);
            return Err(ScmApiError::Unavailable);
        }
        let status = status.assume_init();
        let mut state = map_dw_state(status.dwCurrentState);

        needed = 0;
        QueryServiceConfigW(service, ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            CloseServiceHandle(service);
            CloseServiceHandle(scm);
            return Err(ScmApiError::Unavailable);
        }
        let align = align_of::<QUERY_SERVICE_CONFIGW>();
        let size = needed as usize;
        let mut raw = vec![0u8; size + align];
        let unaligned = raw.as_mut_ptr() as usize;
        let aligned = (unaligned + align - 1) & !(align - 1);
        let config_ptr = aligned as *mut QUERY_SERVICE_CONFIGW;
        let config_ok = QueryServiceConfigW(service, config_ptr, needed, &mut needed);
        let last_cfg_err = windows_sys::Win32::Foundation::GetLastError();
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if config_ok == 0 {
            return Err(if last_cfg_err == ERROR_ACCESS_DENIED {
                ScmApiError::AccessDenied
            } else {
                ScmApiError::Unavailable
            });
        }
        let config = &*config_ptr;
        let image = wide_to_string(config.lpBinaryPathName);
        let start_name = wide_to_string(config.lpServiceStartName);
        let image_token = opaque_image_token(image.trim_matches('"'));
        let execution_identity_token = opaque_image_token(&start_name);
        let start_mode = map_start_type(config.dwStartType);

        let clean_image = image.trim_matches('"');
        let runner_home = Path::new(clean_image).parent().and_then(|p| {
            if p.file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case("bin"))
                .unwrap_or(false)
            {
                p.parent()
            } else {
                Some(p)
            }
        });
        if let Some(home) = runner_home {
            let (listener_present, worker_present, obs_ok) =
                crate::windows_runner_discovery::observe_runner_processes_for_home(home);
            if worker_present {
                state = DiscoveredLifecycle::Busy;
            } else if obs_ok && !listener_present && status.dwProcessId == 0 {
                state = DiscoveredLifecycle::Stopped;
            }
        }

        let _ = ERROR_SERVICE_DOES_NOT_EXIST;
        Ok(ScmExactSnapshot {
            observation: ServiceObservation {
                service_name: service_name.to_owned(),
                state,
                start_mode,
                image_path_matches: !image.trim().is_empty(),
                ownership_verified: !start_name.trim().is_empty(),
            },
            image_token,
            execution_identity_token,
        })
    }
}

/// Real StartServiceW path — only reachable with a privately minted mutation arm.
#[cfg(windows)]
fn windows_start_service(service_name: &str) -> Result<(), ScmApiError> {
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, StartServiceW, SC_MANAGER_CONNECT,
        SERVICE_START,
    };
    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(scm, wide.as_ptr(), SERVICE_START);
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }
        let ok = StartServiceW(service, 0, ptr::null());
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if ok == 0 {
            return Err(ScmApiError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn windows_stop_service(service_name: &str) -> Result<(), ScmApiError> {
    use std::mem::MaybeUninit;
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
        SERVICE_CONTROL_STOP, SERVICE_STATUS, SERVICE_STOP,
    };
    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(scm, wide.as_ptr(), SERVICE_STOP);
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }
        let mut status = MaybeUninit::<SERVICE_STATUS>::zeroed();
        let ok = ControlService(service, SERVICE_CONTROL_STOP, status.as_mut_ptr());
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if ok == 0 {
            return Err(ScmApiError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn windows_change_start_mode(
    service_name: &str,
    mode: ServiceStartMode,
) -> Result<(), ScmApiError> {
    use std::ptr;
    use windows_sys::Win32::System::Services::{
        ChangeServiceConfigW, CloseServiceHandle, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
        SERVICE_CHANGE_CONFIG, SERVICE_NO_CHANGE,
    };
    let start_type = match mode {
        ServiceStartMode::Automatic => windows_sys::Win32::System::Services::SERVICE_AUTO_START,
        ServiceStartMode::Manual => windows_sys::Win32::System::Services::SERVICE_DEMAND_START,
        ServiceStartMode::Disabled => windows_sys::Win32::System::Services::SERVICE_DISABLED,
        ServiceStartMode::Unknown => return Err(ScmApiError::StateUnknown),
    };
    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(map_last_error_open());
        }
        let wide = to_wide(service_name);
        let service = OpenServiceW(scm, wide.as_ptr(), SERVICE_CHANGE_CONFIG);
        if service.is_null() {
            let err = map_last_error_open();
            CloseServiceHandle(scm);
            return Err(err);
        }
        let ok = ChangeServiceConfigW(
            service,
            SERVICE_NO_CHANGE,
            start_type,
            SERVICE_NO_CHANGE,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
        );
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if ok == 0 {
            return Err(ScmApiError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn map_last_error_open() -> ScmApiError {
    use windows_sys::Win32::Foundation::{
        GetLastError, ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST,
    };
    match unsafe { GetLastError() } {
        ERROR_ACCESS_DENIED => ScmApiError::AccessDenied,
        ERROR_SERVICE_DOES_NOT_EXIST => ScmApiError::NotFound,
        _ => ScmApiError::Unavailable,
    }
}

#[cfg(windows)]
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn map_dw_state(state: u32) -> DiscoveredLifecycle {
    use windows_sys::Win32::System::Services::{
        SERVICE_PAUSED, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STOPPED,
        SERVICE_STOP_PENDING,
    };
    match state {
        SERVICE_RUNNING => DiscoveredLifecycle::Listening,
        SERVICE_STOPPED => DiscoveredLifecycle::Stopped,
        SERVICE_START_PENDING | SERVICE_STOP_PENDING | SERVICE_PAUSED => {
            DiscoveredLifecycle::Unknown
        }
        _ => DiscoveredLifecycle::Unknown,
    }
}

#[cfg(windows)]
fn map_start_type(start_type: u32) -> ServiceStartMode {
    use windows_sys::Win32::System::Services::{
        SERVICE_AUTO_START, SERVICE_DEMAND_START, SERVICE_DISABLED,
    };
    match start_type {
        SERVICE_AUTO_START => ServiceStartMode::Automatic,
        SERVICE_DEMAND_START => ServiceStartMode::Manual,
        SERVICE_DISABLED => ServiceStartMode::Disabled,
        _ => ServiceStartMode::Unknown,
    }
}

#[cfg(windows)]
unsafe fn wide_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
}

/// Backward-compatible alias: always mutation-inert.
#[derive(Clone, Copy, Debug, Default)]
pub struct GatedWindowsScmBackend;

impl ScmControlBackend for GatedWindowsScmBackend {
    fn query_exact(&self, service_name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
        WindowsScmBackend::mutation_inert().query_exact(service_name)
    }

    fn start(&self, service_name: &str) -> Result<(), ScmApiError> {
        WindowsScmBackend::mutation_inert().start(service_name)
    }

    fn stop(&self, service_name: &str) -> Result<(), ScmApiError> {
        WindowsScmBackend::mutation_inert().stop(service_name)
    }

    fn change_start_mode(
        &self,
        service_name: &str,
        mode: ServiceStartMode,
    ) -> Result<(), ScmApiError> {
        WindowsScmBackend::mutation_inert().change_start_mode(service_name, mode)
    }
}

/// Strongly typed narrow action supported by the privileged SCM helper.
///
/// Specifically excludes blanket elevation, broad service manager control,
/// foreign service configuration, and generic process termination (kill_pid).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrivilegedScmAction {
    StartService,
    StopService,
    ChangeStartMode { mode: ServiceStartMode },
}

/// Request presented to the privileged SCM helper by the Agent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedScmRequest {
    pub action: PrivilegedScmAction,
    pub service_name: String,
    pub expected_image_token: String,
    pub expected_execution_identity_token: String,
    #[serde(default)]
    pub target_host_token: String,
    #[serde(default)]
    pub candidate_commit: String,
    pub transaction_id: String,
}

/// Verifiable report emitted by the privileged SCM helper.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedScmReport {
    pub service_name: String,
    pub transaction_id: String,
    pub applied: bool,
    pub achieved_state: DiscoveredLifecycle,
    pub achieved_start_mode: ServiceStartMode,
    pub refusal_reason: Option<String>,
}

/// Narrow privileged helper executing strictly scoped Windows SCM mutations.
///
/// Ordinary user-session Agent delegates only exact, single-service actions here.
/// Does not accept wildcards, foreign services, or blanket administrative elevation.
pub struct PrivilegedScmHelper<B> {
    backend: B,
    gate: LiveScmExecutionGate,
    clock: Arc<dyn TrustedClock>,
    target_host_token: String,
    candidate_commit: String,
}

impl<B: ScmControlBackend> PrivilegedScmHelper<B> {
    pub fn new(backend: B, gate: LiveScmExecutionGate) -> Self {
        let target_host_token = gate.bound_target_host_token.clone();
        let candidate_commit = gate.bound_candidate_commit.clone();
        Self {
            backend,
            target_host_token,
            candidate_commit,
            gate,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn TrustedClock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn with_host_and_commit(
        mut self,
        target_host_token: impl Into<String>,
        candidate_commit: impl Into<String>,
    ) -> Self {
        self.target_host_token = target_host_token.into();
        self.candidate_commit = candidate_commit.into();
        self
    }

    pub fn dispatch(&self, request: &PrivilegedScmRequest) -> PrivilegedScmReport {
        let name = request.service_name.trim();
        if name.is_empty()
            || name.contains('*')
            || name.contains('?')
            || name.contains('\\')
            || name.contains('/')
        {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(ScmApiError::NameDrift.as_str().to_owned()),
            };
        }
        if request.transaction_id.trim().is_empty() {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
            };
        }

        // 1. Verify gate permission and complete context for the specific action
        let (need_start, need_stop, need_config) = match request.action {
            PrivilegedScmAction::StartService => (true, false, false),
            PrivilegedScmAction::StopService => (false, true, false),
            PrivilegedScmAction::ChangeStartMode { .. } => (false, false, true),
        };
        if !request.target_host_token.is_empty()
            && request.target_host_token != self.target_host_token
        {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(ScmApiError::IdentityMismatch.as_str().to_owned()),
            };
        }
        if !request.candidate_commit.is_empty() && request.candidate_commit != self.candidate_commit
        {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(ScmApiError::IdentityMismatch.as_str().to_owned()),
            };
        }
        let context = ScmExecutionContext {
            service_name: name,
            image_token: &request.expected_image_token,
            execution_identity_token: &request.expected_execution_identity_token,
            target_host_token: &self.target_host_token,
            candidate_commit: &self.candidate_commit,
            transaction_id: &request.transaction_id,
            need_start,
            need_stop,
            need_config,
        };
        if let Err(err) = self.gate.authorizes_context(&context, self.clock.as_ref()) {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: DiscoveredLifecycle::Unknown,
                achieved_start_mode: ServiceStartMode::Disabled,
                refusal_reason: Some(err.as_str().to_owned()),
            };
        }

        // 2. Query exact host state & verify identity (foreign service refusal)
        let snapshot = match self.backend.query_exact(name) {
            Ok(snap) => snap,
            Err(error) => {
                return PrivilegedScmReport {
                    service_name: request.service_name.clone(),
                    transaction_id: request.transaction_id.clone(),
                    applied: false,
                    achieved_state: DiscoveredLifecycle::Unknown,
                    achieved_start_mode: ServiceStartMode::Disabled,
                    refusal_reason: Some(error.as_str().to_owned()),
                };
            }
        };

        if !snapshot.observation.service_name.eq_ignore_ascii_case(name) {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(ScmApiError::NameDrift.as_str().to_owned()),
            };
        }
        if !snapshot.observation.ownership_verified
            || !snapshot.observation.image_path_matches
            || !snapshot
                .image_token
                .eq_ignore_ascii_case(&request.expected_image_token)
            || !snapshot
                .execution_identity_token
                .eq_ignore_ascii_case(&request.expected_execution_identity_token)
        {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(ScmApiError::IdentityMismatch.as_str().to_owned()),
            };
        }

        // 3. Worker Busy / Unknown check for StopService (fail-closed on active or ambiguous worker)
        if matches!(request.action, PrivilegedScmAction::StopService)
            && (matches!(snapshot.observation.state, DiscoveredLifecycle::Busy)
                || matches!(snapshot.observation.state, DiscoveredLifecycle::Unknown))
        {
            let reason = if matches!(snapshot.observation.state, DiscoveredLifecycle::Busy) {
                ScmApiError::BusyWorker
            } else {
                ScmApiError::StateUnknown
            };
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(reason.as_str().to_owned()),
            };
        }

        // 4. Perform narrow mutation
        if let Err(err) = self.gate.authorizes_context(&context, self.clock.as_ref()) {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(err.as_str().to_owned()),
            };
        }

        let mutate_result = match request.action {
            PrivilegedScmAction::StartService => self.backend.start(name),
            PrivilegedScmAction::StopService => self.backend.stop(name),
            PrivilegedScmAction::ChangeStartMode { mode } => {
                self.backend.change_start_mode(name, mode)
            }
        };
        if let Err(error) = mutate_result {
            return PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: false,
                achieved_state: snapshot.observation.state,
                achieved_start_mode: snapshot.observation.start_mode,
                refusal_reason: Some(error.as_str().to_owned()),
            };
        }

        // 5. Immediate post-action readback
        let after = match self.backend.query_exact(name) {
            Ok(snap) => snap,
            Err(error) => {
                return PrivilegedScmReport {
                    service_name: request.service_name.clone(),
                    transaction_id: request.transaction_id.clone(),
                    applied: true,
                    achieved_state: DiscoveredLifecycle::Unknown,
                    achieved_start_mode: ServiceStartMode::Disabled,
                    refusal_reason: Some(error.as_str().to_owned()),
                };
            }
        };

        // Verify transition succeeded
        let matches_expectation = match request.action {
            PrivilegedScmAction::StartService => matches!(
                after.observation.state,
                DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
            ),
            PrivilegedScmAction::StopService => {
                matches!(after.observation.state, DiscoveredLifecycle::Stopped)
            }
            PrivilegedScmAction::ChangeStartMode { mode } => after.observation.start_mode == mode,
        };

        if !matches_expectation {
            PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: true,
                achieved_state: after.observation.state,
                achieved_start_mode: after.observation.start_mode,
                refusal_reason: Some(ScmApiError::RestoreIncomplete.as_str().to_owned()),
            }
        } else {
            PrivilegedScmReport {
                service_name: request.service_name.clone(),
                transaction_id: request.transaction_id.clone(),
                applied: true,
                achieved_state: after.observation.state,
                achieved_start_mode: after.observation.start_mode,
                refusal_reason: None,
            }
        }
    }
}

pub const SCM_TRANSACTION_JOURNAL_SCHEMA_VERSION: u32 = 1;

/// Durable transaction journal written to resilient private storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScmTransactionJournal {
    pub schema_version: u32,
    pub transaction_id: String,
    pub service_name: String,
    pub expected_image_token: String,
    pub expected_execution_identity_token: String,
    pub prestate: ServicePrestateCapture,
    pub stage: ScmTransactionStage,
    pub restore_plan: ServiceRestorePlan,
    pub updated_at_unix_ms: u64,
    #[serde(default)]
    pub consumed_token_hash: Option<String>,
    #[serde(default)]
    pub consumed_nonce: Option<String>,
}

impl ScmTransactionJournal {
    pub fn new(
        prestate: ServicePrestateCapture,
        expected_image_token: impl Into<String>,
        expected_execution_identity_token: impl Into<String>,
        updated_at_unix_ms: u64,
    ) -> Self {
        let restore_plan = capture_restore_plan(&prestate);
        Self {
            schema_version: SCM_TRANSACTION_JOURNAL_SCHEMA_VERSION,
            transaction_id: prestate.transaction_id.clone(),
            service_name: prestate.service_name.clone(),
            expected_image_token: expected_image_token.into(),
            expected_execution_identity_token: expected_execution_identity_token.into(),
            stage: ScmTransactionStage::CapturedPrestate,
            restore_plan,
            prestate,
            updated_at_unix_ms,
            consumed_token_hash: None,
            consumed_nonce: None,
        }
    }

    pub fn record_consumed_authorization(&mut self, token_hash_hex: String, nonce: String) {
        self.consumed_token_hash = Some(token_hash_hex);
        self.consumed_nonce = Some(nonce);
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.stage,
            ScmTransactionStage::Completed
                | ScmTransactionStage::Refused
                | ScmTransactionStage::RestoreFailed
        )
    }

    pub fn record_stage(&mut self, stage: ScmTransactionStage, updated_at_unix_ms: u64) {
        self.stage = stage;
        self.updated_at_unix_ms = updated_at_unix_ms;
    }

    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        crate::installation::atomic_write(path, &bytes)
    }

    pub fn load_from_path(path: &Path) -> io::Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        if crate::installation::is_reparse_point(path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing reparse journal {}", path.display()),
            ));
        }
        let bytes = fs::read(path)?;
        let journal: Self = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        if journal.schema_version != SCM_TRANSACTION_JOURNAL_SCHEMA_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported journal schema version {}",
                    journal.schema_version
                ),
            ));
        }
        Ok(Some(journal))
    }

    pub fn clear(path: &Path) -> io::Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        if crate::installation::is_reparse_point(path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing reparse journal {}", path.display()),
            ));
        }
        fs::remove_file(path)?;
        Ok(true)
    }
}

/// Reconciles an interrupted transaction journal after an Agent crash or restart.
///
/// CRITICAL SAFETY CONTRACT:
/// A restarted Agent resuming an in-flight journal NEVER marks `restore_pass: true`
/// or reports `Completed` without fresh, independent readbacks confirming that
/// the service state, start mode, AND admission presence truthfully match the restore plan.
pub fn reconcile_interrupted_scm_journal<B: ScmControlBackend, A: AdmissionSelectorObserver>(
    journal: &mut ScmTransactionJournal,
    executor: &ScmControlExecutor<B>,
    admission: &mut A,
    current_time_unix_ms: u64,
) -> ScmTransactionReceipt {
    if let (Some(token_hex), Some(nonce)) = (&journal.consumed_token_hash, &journal.consumed_nonce)
    {
        if let Some(arr) = decode_hex_32(token_hex) {
            let _ = SharedAuthorizationLedger::global().try_consume(arr, nonce);
        }
    }

    let current_observation = match executor.observe_verified() {
        Ok(obs) => obs,
        Err(error) => {
            journal.record_stage(ScmTransactionStage::Interrupted, current_time_unix_ms);
            return ScmTransactionReceipt {
                service_name: journal.service_name.clone(),
                stage: ScmTransactionStage::Interrupted,
                api_accepted: false,
                achieved: AchievedServiceControl::Unknown,
                reason: Some(format!("resume-observe-failed: {}", error.as_str())),
                restore: Some(journal.restore_plan.clone()),
                restore_pass: false,
            };
        }
    };

    let admission_absent = match admission.observe_selector_absent() {
        Ok(absent) => absent,
        Err(error) => {
            journal.record_stage(ScmTransactionStage::Interrupted, current_time_unix_ms);
            return ScmTransactionReceipt {
                service_name: journal.service_name.clone(),
                stage: ScmTransactionStage::Interrupted,
                api_accepted: false,
                achieved: achieved_from_observation(&current_observation, false),
                reason: Some(format!(
                    "resume-admission-observe-failed: {}",
                    error.as_str()
                )),
                restore: Some(journal.restore_plan.clone()),
                restore_pass: false,
            };
        }
    };

    let running_now = matches!(
        current_observation.state,
        DiscoveredLifecycle::Listening | DiscoveredLifecycle::Busy
    );

    // Check if the host is already in the fully restored state
    let state_restored = running_now == journal.restore_plan.restore_running
        && current_observation.start_mode == journal.restore_plan.restore_start_mode
        && admission_absent == journal.restore_plan.admission_selector_absent;

    if state_restored {
        journal.record_stage(ScmTransactionStage::Completed, current_time_unix_ms);
        return ScmTransactionReceipt {
            service_name: journal.service_name.clone(),
            stage: ScmTransactionStage::Completed,
            api_accepted: true,
            achieved: achieved_from_observation(&current_observation, false),
            reason: None,
            restore: Some(journal.restore_plan.clone()),
            restore_pass: true,
        };
    }

    // Otherwise, attempt recovery by executing the bound restore plan
    journal.record_stage(ScmTransactionStage::Restoring, current_time_unix_ms);
    let receipt = executor.restore(&journal.restore_plan, admission);
    journal.record_stage(receipt.stage, current_time_unix_ms);
    receipt
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticAdmission {
        absent: bool,
        fail: Option<ScmApiError>,
    }

    impl AdmissionSelectorObserver for StaticAdmission {
        fn observe_selector_absent(&mut self) -> Result<bool, ScmApiError> {
            if let Some(error) = self.fail.take() {
                return Err(error);
            }
            Ok(self.absent)
        }
    }

    fn fixture_listening() -> MockServiceRecord {
        MockServiceRecord {
            observation: ServiceObservation {
                service_name: "fixture.scm.a".to_owned(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
        }
    }

    fn open_synthetic_gate() -> LiveScmExecutionGate {
        LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
            "fixture.scm.a",
            "fixture-image",
            true,
            true,
            true,
        ))
        .unwrap()
    }

    fn executor(
        mock: MockScmBackend,
        gate: LiveScmExecutionGate,
    ) -> ScmControlExecutor<MockScmBackend> {
        ScmControlExecutor::new(
            mock,
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            gate,
        )
    }

    #[test]
    fn observe_verified_does_not_overwrite_false_trust_flags() {
        let mock = MockScmBackend::new();
        let mut record = fixture_listening();
        record.observation.ownership_verified = false;
        mock.insert(record);
        let exec = executor(mock, LiveScmExecutionGate::CLOSED);
        assert!(matches!(
            exec.observe_verified(),
            Err(ScmApiError::IdentityMismatch)
        ));
    }

    #[test]
    fn image_token_mismatch_is_refused() {
        let mock = MockScmBackend::new();
        let mut record = fixture_listening();
        record.image_token = "other-image".to_owned();
        mock.insert(record);
        let exec = executor(mock, LiveScmExecutionGate::CLOSED);
        assert!(matches!(
            exec.observe_verified(),
            Err(ScmApiError::IdentityMismatch)
        ));
    }

    #[test]
    fn unissued_trusted_verifier_never_opens_live_gate() {
        let request = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-unissued-test".to_owned(),
            owner_material: b"not-a-real-token".to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "unissued-test-nonce".to_owned(),
        };
        assert!(matches!(
            LiveScmExecutionGate::from_trusted_owner_authorization(
                &UnissuedOwnerAuthorizationVerifier,
                &request
            ),
            Err(ScmApiError::AuthorizationNotIssued)
        ));
    }

    #[test]
    fn synthetic_grant_is_marked_test_only() {
        let gate = open_synthetic_gate();
        assert!(gate.is_synthetic_test_only());
        assert!(!gate.is_trusted_live());
    }

    #[test]
    fn synthetic_grant_cannot_mutate_gated_windows_backend() {
        // Forgeable synthetic grants open mock/synthetic gates only. The gated
        // Windows adapter remains mutation-inert regardless of grant kind.
        let gate = open_synthetic_gate();
        assert!(gate.is_synthetic_test_only());
        let backend = GatedWindowsScmBackend;
        assert!(matches!(
            backend.start("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.stop("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.change_start_mode("fixture.scm.a", ServiceStartMode::Manual),
            Err(ScmApiError::GateClosed)
        ));
    }

    #[test]
    fn closed_gate_refuses_every_mutation_intent_and_reports_closed() {
        let gate = LiveScmExecutionGate::CLOSED;
        assert!(!gate.allows_mutation());
        assert!(!gate.is_synthetic_test_only());
        assert!(!gate.is_trusted_live());
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", true, false, false));
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, true, false));
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, false, true));
    }

    #[test]
    fn granular_gate_permissions_and_identity_match_strictly_enforced() {
        let grant = SyntheticScmTestGrant::new(
            "fixture.scm.a",
            "fixture-image",
            true,  // allow_start
            false, // allow_stop
            false, // allow_change_config
        );
        let gate = LiveScmExecutionGate::from_synthetic_test_grant(&grant).unwrap();
        assert!(gate.allows_mutation());
        assert!(gate.authorizes("fixture.scm.a", "fixture-image", true, false, false));
        // Disallowed stop
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, true, false));
        // Disallowed config
        assert!(!gate.authorizes("fixture.scm.a", "fixture-image", false, false, true));
        // Service mismatch
        assert!(!gate.authorizes("other.service", "fixture-image", true, false, false));
        // Image token mismatch
        assert!(!gate.authorizes("fixture.scm.a", "other-image", true, false, false));
    }

    #[test]
    fn unissued_verifier_rejects_diverse_request_payloads() {
        let verifier = UnissuedOwnerAuthorizationVerifier;
        let empty_req = TrustedOwnerAuthorizationRequest {
            service_name: String::new(),
            image_token: String::new(),
            execution_identity_token: String::new(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            allow_start: false,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: String::new(),
            owner_material: Vec::new(),
            not_before_epoch_seconds: 0,
            expires_at_epoch_seconds: 0,
            nonce: String::new(),
        };
        assert!(matches!(
            verifier.verify(&empty_req),
            Err(ScmApiError::AuthorizationNotIssued)
        ));

        let full_req = TrustedOwnerAuthorizationRequest {
            service_name: "legitimate.service".to_owned(),
            image_token: "legitimate-token".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: vec![1, 2, 3, 4, 5],
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-1".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&full_req),
            Err(ScmApiError::AuthorizationNotIssued)
        ));
    }

    #[test]
    fn trusted_owner_verifier_valid_token_opens_trusted_live_gate() {
        let authority_secret = b"secret-token-valid-opens-gate-suite";
        let clock = Arc::new(MockClock::new(1_500));
        let scope = OwnerAuthorizationScope {
            allow_change_config: true,
            ..OwnerAuthorizationScope::test_fixture("fixture.scm.a", "fixture-image")
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, clock.clone());
        let (request, _voucher) = issuer.issue_for_scope(&scope, 500, "nonce-valid");
        let verifier =
            TokenValidatingOwnerVerifier::with_authority_and_clock(authority_secret, scope, clock);
        let gate =
            LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request).unwrap();
        assert!(gate.is_trusted_live());
        assert!(!gate.is_synthetic_test_only());
        assert!(gate.allows_mutation());
        assert!(gate.authorizes("fixture.scm.a", "fixture-image", true, false, false));
        assert!(gate.authorizes("fixture.scm.a", "fixture-image", false, true, false));
        assert!(gate.authorizes("fixture.scm.a", "fixture-image", false, false, true));
        assert!(!gate.authorizes("other.service", "fixture-image", true, false, false));
        assert!(!gate.authorizes("fixture.scm.a", "other-image", true, false, false));
        assert_eq!(verifier.consumed_nonce_count(), 1);
    }

    #[test]
    fn trusted_owner_verifier_refusal_paths() {
        let authority_secret = b"secret-token-refusal-paths-suite-42";
        let mock_clock = Arc::new(MockClock::new(1_500));
        let scope = OwnerAuthorizationScope {
            service_name: "fixture.scm.a".to_string(),
            image_token: "fixture-image".to_string(),
            execution_identity_token: "fixture-identity-token".to_string(),
            target_host_token: "fixture-host-token".to_string(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_string(),
            transaction_id: "tx-fixture-1".to_string(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, mock_clock.clone());
        let (_base_req, raw_token) = issuer.issue_for_scope(&scope, 500, "nonce-base");
        let verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope.clone(),
            mock_clock.clone(),
        );

        // 1. Empty owner material refuses with AuthorizationNotIssued
        let empty_mat_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: Vec::new(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-1".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&empty_mat_req),
            Err(ScmApiError::AuthorizationNotIssued)
        ));

        // 2. Tampered / invalid token refuses with AccessDenied
        let tampered_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: b"tampered-forged-token".to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-2".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&tampered_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 3. Wildcard service name refuses with NameDrift
        let wildcard_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.*".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-3".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&wildcard_req),
            Err(ScmApiError::NameDrift)
        ));

        // 4. Service name drift refuses with NameDrift
        let name_drift_req = TrustedOwnerAuthorizationRequest {
            service_name: "other.service.name".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-4".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&name_drift_req),
            Err(ScmApiError::NameDrift)
        ));

        // 5. Image token mismatch refuses with IdentityMismatch
        let image_mismatch_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "different-image-token".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-5".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&image_mismatch_req),
            Err(ScmApiError::IdentityMismatch)
        ));

        // 6. Execution identity token mismatch refuses with IdentityMismatch
        let id_mismatch_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "wrong-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-id".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&id_mismatch_req),
            Err(ScmApiError::IdentityMismatch)
        ));

        // 7. Target host token mismatch refuses with AccessDenied
        let host_mismatch_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "wrong-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-host".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&host_mismatch_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 8. Candidate commit mismatch refuses with AccessDenied
        let commit_mismatch_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-commit".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&commit_mismatch_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 9. Transaction ID mismatch refuses with AccessDenied
        let tx_mismatch_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-wrong-id".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-tx".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&tx_mismatch_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 10. Excessive permission request (verifier only allows start/stop, request asks for change config)
        let limited_scope = OwnerAuthorizationScope {
            allow_change_config: false,
            ..scope.clone()
        };
        let limited_verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            limited_scope,
            mock_clock.clone(),
        );
        let excessive_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-refusal-6".to_owned(),
        };
        assert!(matches!(
            limited_verifier.verify(&excessive_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 11. Empty nonce refuses with AccessDenied
        let empty_nonce_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "   ".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&empty_nonce_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 12. Inverted validity window (expires_at <= not_before) refuses with AccessDenied
        let inverted_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 2_000,
            expires_at_epoch_seconds: 1_000,
            nonce: "nonce-inverted".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&inverted_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 13. Expired token against trusted clock (now >= expires_at) refuses with AccessDenied
        mock_clock.set_time(2_000);
        let expired_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-expired".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&expired_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 14. Not-yet-valid token against trusted clock (now < not_before) refuses with AccessDenied
        mock_clock.set_time(999);
        let premature_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: raw_token.to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "nonce-premature".to_owned(),
        };
        assert!(matches!(
            verifier.verify(&premature_req),
            Err(ScmApiError::AccessDenied)
        ));

        // Restore clock to valid window
        mock_clock.set_time(1_500);

        // 15. Valid first request opens TrustedLive gate and consumes authorization
        let (valid_first_req, _) = issuer.issue_for_scope(&scope, 500, "single-use-nonce-12345");
        let gate1 = verifier.verify(&valid_first_req).unwrap();
        assert!(gate1.is_trusted_live());

        // 16. Replay with identical nonce must fail closed
        assert!(matches!(
            verifier.verify(&valid_first_req),
            Err(ScmApiError::AccessDenied)
        ));

        // 17. Replay with swapped nonce (using a different nonce for the same token) must be refused
        let mut replay_swapped_nonce = valid_first_req.clone();
        replay_swapped_nonce.nonce = "different-nonce-for-same-token".to_owned();
        assert!(matches!(
            verifier.verify(&replay_swapped_nonce),
            Err(ScmApiError::AccessDenied)
        ));

        // 18. Replay with a newly constructed verifier instance must be refused via shared ledger
        let verifier2 = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope.clone(),
            mock_clock.clone(),
        );
        let mut replay_reconstructed_verifier = valid_first_req.clone();
        replay_reconstructed_verifier.nonce = "nonce-to-fresh-verifier".to_owned();
        assert!(matches!(
            verifier2.verify(&replay_reconstructed_verifier),
            Err(ScmApiError::AccessDenied)
        ));

        // 19. Authority voucher issuance and verification with self-selected refusal
        let issuer = OwnerAuthorizationIssuer::with_clock(
            b"trusted-authority-master-secret",
            mock_clock.clone(),
        );
        let (issued_req, _issued_token) = issuer.issue(
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity-token",
            "fixture-host-token",
            "028c7efba9ebd3d3925371b356a187eba428041d",
            true,
            true,
            true,
            "tx-fixture-1",
            500,
            "issued-nonce-19",
        );
        let authority_verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            b"trusted-authority-master-secret",
            scope,
            mock_clock,
        );
        let issued_gate = authority_verifier.verify(&issued_req).unwrap();
        assert!(issued_gate.is_trusted_live());

        let self_selected_req = TrustedOwnerAuthorizationRequest {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-fixture-1".to_owned(),
            owner_material: b"caller-arbitrary-chosen-token".to_vec(),
            not_before_epoch_seconds: 1_000,
            expires_at_epoch_seconds: 2_000,
            nonce: "self-selected-nonce".to_owned(),
        };
        assert!(matches!(
            authority_verifier.verify(&self_selected_req),
            Err(ScmApiError::AccessDenied)
        ));
    }

    #[test]
    fn trusted_owner_verifier_concurrent_race_allows_exactly_one_success() {
        use std::sync::Barrier;
        use std::thread;

        let authority_secret = b"owner-root-secret-concurrent-race-internal";
        let scope = OwnerAuthorizationScope::test_fixture("fixture.scm.a", "fixture-image");
        let clock = Arc::new(MockClock::new(1_500));
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, clock.clone());
        let verifier = Arc::new(TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope.clone(),
            clock,
        ));

        let (request, _) = issuer.issue_for_scope(&scope, 3600, "concurrent-race-internal-nonce");
        let request = Arc::new(request);

        let thread_count = 8;
        let barrier = Arc::new(Barrier::new(thread_count));
        let mut handles = Vec::new();

        for _ in 0..thread_count {
            let v = Arc::clone(&verifier);
            let req = Arc::clone(&request);
            let b = Arc::clone(&barrier);

            handles.push(thread::spawn(move || {
                b.wait();
                v.verify(&req)
            }));
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let denial_count = results
            .iter()
            .filter(|r| matches!(r, Err(ScmApiError::AccessDenied)))
            .count();

        assert_eq!(success_count, 1, "Exactly one thread must succeed");
        assert_eq!(
            denial_count,
            thread_count - 1,
            "All other racing threads must receive AccessDenied"
        );
    }

    #[test]
    fn trusted_live_gate_defense_in_depth_windows_backend_stays_inert() {
        let authority_secret = b"authority-secret-defense-in-depth-inert";
        let clock = Arc::new(MockClock::new(1_500));
        let scope = OwnerAuthorizationScope {
            allow_change_config: true,
            ..OwnerAuthorizationScope::test_fixture("fixture.scm.a", "fixture-image")
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, clock.clone());
        let (request, _voucher) = issuer.issue_for_scope(&scope, 1_000, "defense-in-depth-nonce");
        let verifier =
            TokenValidatingOwnerVerifier::with_authority_and_clock(authority_secret, scope, clock);
        let gate =
            LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request).unwrap();
        assert!(gate.is_trusted_live());

        let backend = WindowsScmBackend::mutation_inert();
        assert!(matches!(
            backend.start("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.stop("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.change_start_mode("fixture.scm.a", ServiceStartMode::Manual),
            Err(ScmApiError::GateClosed)
        ));

        let gated_backend = GatedWindowsScmBackend;
        assert!(matches!(
            gated_backend.start("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            gated_backend.stop("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            gated_backend.change_start_mode("fixture.scm.a", ServiceStartMode::Manual),
            Err(ScmApiError::GateClosed)
        ));
    }

    #[test]
    fn trusted_live_gate_controls_mock_backend_on_exact_identity() {
        let authority_secret = b"authority-secret-controls-mock-exact";
        let clock = Arc::new(MockClock::new(1_500));
        let scope = OwnerAuthorizationScope {
            allow_change_config: true,
            ..OwnerAuthorizationScope::test_fixture("fixture.scm.a", "fixture-image")
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, clock.clone());
        let (request, _voucher) = issuer.issue_for_scope(&scope, 1_000, "mock-exact-nonce");
        let verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope,
            clock.clone(),
        );
        let gate =
            LiveScmExecutionGate::from_trusted_owner_authorization(&verifier, &request).unwrap();
        assert!(gate.is_trusted_live());

        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), gate).with_clock(clock);
        let prestate = exec.capture_prestate(false, true).unwrap();
        let receipt = exec.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::StopReadback);
        assert_eq!(receipt.achieved, AchievedServiceControl::Stopped);
        assert_eq!(mock.stop_calls(), 1);
    }

    #[test]
    fn closed_gate_refuses_stop_without_mutation() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), LiveScmExecutionGate::CLOSED);
        let prestate = exec.capture_prestate(false, true).unwrap();
        let receipt = exec.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.reason.as_deref(), Some("scm.live-gate-closed"));
        assert_eq!(mock.stop_calls(), 0);
        assert!(!receipt.restore_pass);
    }

    #[test]
    fn open_gate_stop_readback_reports_stopped() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let receipt = exec.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::StopReadback);
        assert_eq!(receipt.achieved, AchievedServiceControl::Stopped);
        assert_eq!(mock.stop_calls(), 1);
    }

    #[test]
    fn stop_accepted_but_readback_failure_is_interrupted_not_restore_pass() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        // capture_prestate already queried once. Allow one more successful query
        // (advance planning observe), then fail the post-stop readback.
        mock.set_fail_after_queries(1, ScmApiError::Timeout);
        let receipt = exec.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::Interrupted);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.achieved, AchievedServiceControl::Unknown);
        assert_eq!(receipt.reason.as_deref(), Some("scm.timeout"));
        assert_eq!(mock.stop_calls(), 1);
    }

    #[test]
    fn start_accepted_but_readback_failure_is_interrupted_not_restore_pass() {
        let mock = MockScmBackend::new();
        let mut stopped = fixture_listening();
        stopped.observation.state = DiscoveredLifecycle::Stopped;
        mock.insert(stopped);
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        mock.set_fail_after_queries(1, ScmApiError::Unavailable);
        let receipt = exec.advance(
            DesiredServiceLifecycle::RemainRunning,
            true,
            false,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::Interrupted);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.achieved, AchievedServiceControl::Unknown);
        assert_eq!(receipt.reason.as_deref(), Some("scm.unavailable"));
        assert_eq!(mock.start_calls(), 1);
    }

    #[test]
    fn restore_requires_independent_admission_observation() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
        assert!(receipt.restore_pass);
        assert_eq!(receipt.stage, ScmTransactionStage::Completed);
    }

    #[test]
    fn restore_fails_when_admission_observer_disagrees() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        // Plan expects absent=true; observer still sees present.
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        let receipt = exec.restore(&plan, &mut admission);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
    }

    #[test]
    fn restore_fails_when_admission_observe_errors() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let mut admission = StaticAdmission {
            absent: true,
            fail: Some(ScmApiError::Unavailable),
        };
        let receipt = exec.restore(&plan, &mut admission);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.reason.as_deref(), Some("scm.unavailable"));
    }

    #[test]
    fn restore_fails_when_change_config_errors() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        mock.set_fail_change(ScmApiError::AccessDenied);
        let exec = executor(mock, open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.reason.as_deref(), Some("scm.access-denied"));
    }

    #[test]
    fn restore_start_mode_accepted_but_readback_fails_is_restore_failed_not_restore_pass() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(false, true).unwrap();
        let plan = capture_restore_plan(&prestate);
        // capture_prestate already queried once. Fail on the post-change_start_mode query.
        mock.set_fail_after_queries(1, ScmApiError::Timeout);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
        assert!(receipt.api_accepted);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.achieved, AchievedServiceControl::Unknown);
        assert_eq!(receipt.reason.as_deref(), Some("scm.timeout"));
        assert_eq!(mock.change_calls(), 1);
    }

    #[test]
    fn restore_rejects_plan_for_different_service_binding() {
        let mock = MockScmBackend::new();
        mock.insert(fixture_listening());
        let exec = executor(mock, open_synthetic_gate());
        let mut prestate = exec.capture_prestate(false, true).unwrap();
        prestate.service_name = "fixture.scm.other".to_owned();
        let plan = capture_restore_plan(&prestate);
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt = exec.restore(&plan, &mut admission);
        assert!(!receipt.restore_pass);
        assert_eq!(receipt.stage, ScmTransactionStage::RestoreFailed);
    }

    #[test]
    fn busy_worker_never_issues_stop() {
        let mock = MockScmBackend::new();
        let mut record = fixture_listening();
        record.worker_busy = true;
        record.observation.state = DiscoveredLifecycle::Busy;
        mock.insert(record);
        let exec = executor(mock.clone(), open_synthetic_gate());
        let prestate = exec.capture_prestate(true, true).unwrap();
        let receipt = exec.advance(
            DesiredServiceLifecycle::DrainThenStop,
            true,
            true,
            true,
            &prestate,
        );
        assert_eq!(receipt.stage, ScmTransactionStage::WaitingIdle);
        assert_eq!(mock.stop_calls(), 0);
    }

    #[test]
    fn windows_backend_mutations_are_inert_without_arm() {
        let backend = WindowsScmBackend::mutation_inert();
        assert!(matches!(
            backend.start("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.stop("fixture.scm.a"),
            Err(ScmApiError::GateClosed)
        ));
        assert!(matches!(
            backend.change_start_mode("fixture.scm.a", ServiceStartMode::Manual),
            Err(ScmApiError::GateClosed)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_readonly_query_eventlog_returns_identity() {
        let backend = WindowsScmBackend::mutation_inert();
        let snap = backend
            .query_exact("EventLog")
            .expect("EventLog should be queryable read-only");
        assert!(snap
            .observation
            .service_name
            .eq_ignore_ascii_case("EventLog"));
        assert!(snap.observation.image_path_matches);
        assert!(snap.observation.ownership_verified);
        assert!(!snap.image_token.is_empty());
        assert!(!snap.execution_identity_token.is_empty());
        assert!(!matches!(
            snap.observation.state,
            DiscoveredLifecycle::Unknown
        ));
        assert!(!matches!(
            snap.observation.start_mode,
            ServiceStartMode::Unknown
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_readonly_query_missing_service_is_not_found() {
        let backend = WindowsScmBackend::mutation_inert();
        assert!(matches!(
            backend.query_exact("runnermesh.fixture.missing.service.zz"),
            Err(ScmApiError::NotFound)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_readonly_query_rejects_wildcard_name() {
        let backend = WindowsScmBackend::mutation_inert();
        assert!(matches!(
            backend.query_exact("Event*"),
            Err(ScmApiError::NameDrift)
        ));
    }

    #[test]
    fn fake_github_permission_denial_and_stale_token_fail_closed() {
        let admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_fail_next_withdraw(ScmApiError::AccessDenied);
        assert!(matches!(
            admission.withdraw_capacity(),
            Err(ScmApiError::AccessDenied)
        ));
        assert_eq!(admission.withdraw_calls(), 0);
        // Second attempt succeeds because fail-next was consumed.
        admission.withdraw_capacity().expect("withdraw");
        assert_eq!(admission.withdraw_calls(), 1);

        let stale = FakeGithubAdmissionBackend::new_with_selector_present(true);
        stale.set_stale_token(true);
        assert!(matches!(
            stale.withdraw_capacity(),
            Err(ScmApiError::AccessDenied)
        ));
        let mut observer = FakeGithubAdmissionBackend::new_with_selector_present(false);
        observer.set_stale_token(true);
        assert!(matches!(
            observer.observe_selector_absent(),
            Err(ScmApiError::AccessDenied)
        ));
    }

    #[test]
    fn fake_github_eventual_visibility_lags_before_readback() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_eventual_visibility_lag(1);
        admission.withdraw_capacity().expect("withdraw");
        // First observe still sees selector present (lag).
        assert!(!admission.observe_selector_absent().expect("obs1"));
        // Second observe sees absence after lag consumed.
        assert!(admission.observe_selector_absent().expect("obs2"));
        assert_eq!(admission.observe_calls(), 2);
    }

    #[test]
    fn fake_github_partial_restore_failure_preserves_withdrawn_intent() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.withdraw_capacity().expect("withdraw");
        admission.set_fail_next_restore(ScmApiError::Unavailable);
        assert!(matches!(
            admission.restore_capacity(),
            Err(ScmApiError::Unavailable)
        ));
        assert_eq!(admission.restore_calls(), 0);
        // Withdrawal remains observed; restore did not partially flip presence.
        assert!(admission.observe_selector_absent().expect("absent"));
    }

    #[test]
    fn fake_github_link_loss_during_readback_is_unavailable_not_false_absent() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.withdraw_capacity().expect("withdraw");
        admission.set_disconnected(true);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::Unavailable)
        ));
        // Reconnect must not invent absence without a fresh successful observe.
        admission.set_disconnected(false);
        assert!(admission
            .observe_selector_absent()
            .expect("absent after reconnect"));
    }

    #[test]
    fn fake_github_identity_or_name_drift_observe_fails_closed() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        admission.set_fail_next_observe(ScmApiError::IdentityMismatch);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::IdentityMismatch)
        ));
        admission.set_fail_next_observe(ScmApiError::NameDrift);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::NameDrift)
        ));
        // After drift errors clear, honest absence remains observable.
        assert!(admission.observe_selector_absent().expect("absent"));
    }

    #[test]
    fn fake_github_racing_observe_error_during_lag_does_not_claim_absence() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_eventual_visibility_lag(1);
        admission.withdraw_capacity().expect("withdraw");
        admission.set_fail_next_observe(ScmApiError::AccessDenied);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::AccessDenied)
        ));
        // Lag still applies: next successful observe remains non-absent.
        assert!(!admission.observe_selector_absent().expect("still present"));
        // Only after remaining lag is consumed does absence become visible.
        assert!(admission.observe_selector_absent().expect("absent"));
    }

    #[test]
    fn fake_github_scope_drift_refuses_mutation_and_observe_without_rewriting_presence() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_scope_tokens("scope-a", "scope-b");
        assert!(matches!(
            admission.withdraw_capacity(),
            Err(ScmApiError::IdentityMismatch)
        ));
        assert_eq!(admission.withdraw_calls(), 0);
        assert!(matches!(
            admission.restore_capacity(),
            Err(ScmApiError::IdentityMismatch)
        ));
        assert_eq!(admission.restore_calls(), 0);
        assert!(matches!(
            admission.observe_selector_absent(),
            Err(ScmApiError::IdentityMismatch)
        ));
        // Presence was never rewritten by the refused mutations.
        admission.set_scope_tokens("scope-a", "scope-a");
        assert!(!admission.observe_selector_absent().expect("still present"));
    }

    #[test]
    fn fake_github_racing_restore_during_withdraw_lag_keeps_latest_pending() {
        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        admission.set_eventual_visibility_lag(2);
        admission.withdraw_capacity().expect("withdraw");
        // First observe consumes one lag slot; selector still appears present.
        assert!(!admission.observe_selector_absent().expect("lag1"));
        // Racing restore overwrites the pending assignment to "present".
        admission.restore_capacity().expect("restore during lag");
        assert_eq!(admission.restore_calls(), 1);
        // Remaining lag still hides the pending flip.
        assert!(!admission.observe_selector_absent().expect("lag2"));
        // After lag settles, the latest assignment (restore/present) wins —
        // never a false absence claim from the superseded withdraw.
        assert!(!admission
            .observe_selector_absent()
            .expect("restored present"));
        assert_eq!(admission.observe_calls(), 3);
    }

    fn test_fixture_record(
        name: &str,
        image_token: &str,
        identity_token: &str,
        state: DiscoveredLifecycle,
        start_mode: ServiceStartMode,
        busy: bool,
    ) -> MockServiceRecord {
        MockServiceRecord {
            observation: ServiceObservation {
                service_name: name.to_owned(),
                state,
                start_mode,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: busy,
            image_token: image_token.to_owned(),
            execution_identity_token: identity_token.to_owned(),
        }
    }

    fn test_synthetic_gate(
        service_name: &str,
        image_token: &str,
        allow_start: bool,
        allow_stop: bool,
        allow_change_config: bool,
    ) -> LiveScmExecutionGate {
        LiveScmExecutionGate::from_synthetic_test_grant(&SyntheticScmTestGrant::new(
            service_name,
            image_token,
            allow_start,
            allow_stop,
            allow_change_config,
        ))
        .unwrap()
    }

    #[test]
    fn privileged_scm_helper_enforces_narrow_actions_and_refuses_wildcards_and_foreign_services() {
        let backend = MockScmBackend::new();
        backend.insert(test_fixture_record(
            "fixture.runner.svc",
            "hux2-image",
            "hux2-identity",
            DiscoveredLifecycle::Stopped,
            ServiceStartMode::Manual,
            false,
        ));

        let full_gate = test_synthetic_gate("fixture.runner.svc", "hux2-image", true, true, true);
        let helper = PrivilegedScmHelper::new(backend.clone(), full_gate);

        // 1. Refuse wildcard service name
        let report_wildcard = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.*".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "tx-1".to_owned(),
        });
        assert!(!report_wildcard.applied);
        assert_eq!(
            report_wildcard.refusal_reason.as_deref(),
            Some("scm.name-drift")
        );

        // 2. Refuse empty transaction ID
        let report_empty_tx = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "".to_owned(),
        });
        assert!(!report_empty_tx.applied);
        assert_eq!(
            report_empty_tx.refusal_reason.as_deref(),
            Some("scm.restore-incomplete")
        );

        // 3. Refuse foreign service (image token mismatch)
        let report_foreign = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "wrong-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "tx-2".to_owned(),
        });
        assert!(!report_foreign.applied);
        assert_eq!(
            report_foreign.refusal_reason.as_deref(),
            Some("scm.live-gate-closed") // Gate mismatch because expected_image_token diverged
        );

        // 4. Refuse when gate lacks permission (Start-only gate refuses StopService)
        let start_only_gate =
            test_synthetic_gate("fixture.runner.svc", "hux2-image", true, false, false);
        let helper_start_only = PrivilegedScmHelper::new(backend.clone(), start_only_gate);
        let report_denied_stop = helper_start_only.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StopService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "tx-3".to_owned(),
        });
        assert!(!report_denied_stop.applied);
        assert_eq!(
            report_denied_stop.refusal_reason.as_deref(),
            Some("scm.live-gate-closed")
        );

        // 5. Refuse StopService when worker is busy
        backend.set_worker_busy("fixture.runner.svc", true);
        let report_busy = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StopService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "tx-4".to_owned(),
        });
        assert!(!report_busy.applied);
        assert_eq!(
            report_busy.refusal_reason.as_deref(),
            Some("scm.busy-worker")
        );
        backend.set_worker_busy("fixture.runner.svc", false);

        // 6. Valid StartService succeeds and verifies post-mutation readback
        let report_start = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "tx-5".to_owned(),
        });
        assert!(report_start.applied);
        assert_eq!(report_start.achieved_state, DiscoveredLifecycle::Listening);
        assert_eq!(report_start.refusal_reason, None);

        // 7. Valid ChangeStartMode succeeds
        let report_mode = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::ChangeStartMode {
                mode: ServiceStartMode::Automatic,
            },
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "tx-6".to_owned(),
        });
        assert!(report_mode.applied);
        assert_eq!(report_mode.achieved_start_mode, ServiceStartMode::Automatic);
        assert_eq!(report_mode.refusal_reason, None);
    }

    #[test]
    fn privileged_scm_helper_requires_immediate_readback_confirmation() {
        let backend = MockScmBackend::new();
        backend.insert(test_fixture_record(
            "fixture.runner.svc",
            "hux2-image",
            "hux2-identity",
            DiscoveredLifecycle::Stopped,
            ServiceStartMode::Manual,
            false,
        ));

        let full_gate = test_synthetic_gate("fixture.runner.svc", "hux2-image", true, true, true);
        let helper = PrivilegedScmHelper::new(backend.clone(), full_gate);

        // Allow pre-mutation query to succeed, then fail immediate readback after start
        backend.set_fail_after_queries(1, ScmApiError::Timeout);
        let report = helper.dispatch(&PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.runner.svc".to_owned(),
            expected_image_token: "hux2-image".to_owned(),
            expected_execution_identity_token: "hux2-identity".to_owned(),
            target_host_token: String::new(),
            candidate_commit: String::new(),
            transaction_id: "tx-timeout".to_owned(),
        });
        assert!(report.applied);
        assert_eq!(report.refusal_reason.as_deref(), Some("scm.timeout"));
    }

    #[test]
    fn scm_transaction_journal_atomic_roundtrip_and_terminal_detection() {
        let root =
            std::env::temp_dir().join(format!("runnermesh-test-journal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("test-scm-journal.json");

        let prestate = ServicePrestateCapture {
            service_name: "fixture.runner.svc".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Automatic,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: false,
            image_token: "hux2-image".to_owned(),
            execution_identity_token: "hux2-identity".to_owned(),
            transaction_id: "tx-journal-1".to_owned(),
        };

        let mut journal = ScmTransactionJournal::new(prestate, "hux2-image", "hux2-identity", 1000);
        assert!(!journal.is_terminal());
        assert_eq!(journal.stage, ScmTransactionStage::CapturedPrestate);

        journal.save_to_path(&path).unwrap();

        let loaded = ScmTransactionJournal::load_from_path(&path)
            .unwrap()
            .expect("loaded");
        assert_eq!(loaded, journal);

        journal.record_stage(ScmTransactionStage::Completed, 2000);
        assert!(journal.is_terminal());

        let cleared = ScmTransactionJournal::clear(&path).unwrap();
        assert!(cleared);
        assert!(ScmTransactionJournal::load_from_path(&path)
            .unwrap()
            .is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn interrupted_scm_journal_requires_independent_readback_and_never_assumes_restore_pass() {
        let mock = MockScmBackend::new();
        mock.insert(test_fixture_record(
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            DiscoveredLifecycle::Stopped, // Diverged: was Listening in prestate
            ServiceStartMode::Automatic,
            false,
        ));

        let prestate = ServicePrestateCapture {
            service_name: "fixture.scm.a".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Automatic,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: false,
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            transaction_id: "tx-interrupted-1".to_owned(),
        };

        let mut journal =
            ScmTransactionJournal::new(prestate, "fixture-image", "fixture-identity", 1000);
        journal.record_stage(ScmTransactionStage::RequestedStop, 1500);

        let mut admission = FakeGithubAdmissionBackend::new_with_selector_present(true);
        let gate = LiveScmExecutionGate::CLOSED;
        let exec = executor(mock.clone(), gate);

        // Scenario A: Gate CLOSED and host diverged -> restore fails closed, restore_pass=false
        let receipt_refused =
            reconcile_interrupted_scm_journal(&mut journal, &exec, &mut admission, 2000);
        assert_eq!(receipt_refused.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt_refused.restore_pass);
        assert_eq!(
            receipt_refused.reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(journal.stage, ScmTransactionStage::RestoreFailed);

        // Scenario B: Query readback fails -> Interrupted, restore_pass=false
        mock.set_fail_next(ScmApiError::Unavailable);
        let receipt_unavailable =
            reconcile_interrupted_scm_journal(&mut journal, &exec, &mut admission, 2500);
        assert_eq!(receipt_unavailable.stage, ScmTransactionStage::Interrupted);
        assert!(!receipt_unavailable.restore_pass);
        assert_eq!(journal.stage, ScmTransactionStage::Interrupted);

        // Scenario C: Gate opens, restore succeeds and fresh readbacks confirm restored state
        let open_gate = open_synthetic_gate();
        let exec_open = executor(mock.clone(), open_gate);
        let receipt_success =
            reconcile_interrupted_scm_journal(&mut journal, &exec_open, &mut admission, 3000);
        assert_eq!(receipt_success.stage, ScmTransactionStage::Completed);
        assert!(receipt_success.restore_pass);
        assert_eq!(journal.stage, ScmTransactionStage::Completed);
    }

    fn parse_hex_test(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn hmac_sha256_rfc4231_standard_known_answer_tests() {
        // Test Case 1: Key = [0x0b; 20], Data = "Hi There"
        let key1 = [0x0b; 20];
        let data1 = b"Hi There";
        let expected1 =
            parse_hex_test("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
        let mut mac1 = HmacSha256::new_from_slice(&key1).unwrap();
        mac1.update(data1);
        let digest1 = mac1.finalize().into_bytes();
        assert_eq!(digest1.as_slice(), expected1.as_slice());
        // Constant-time verify_slice
        let mut v1 = HmacSha256::new_from_slice(&key1).unwrap();
        v1.update(data1);
        assert!(v1.verify_slice(&expected1).is_ok());
        // Altered byte fails
        let mut bad_digest1 = expected1.clone();
        bad_digest1[0] ^= 0x01;
        let mut v1_bad = HmacSha256::new_from_slice(&key1).unwrap();
        v1_bad.update(data1);
        assert!(v1_bad.verify_slice(&bad_digest1).is_err());

        // Test Case 2: Key = "Jefe", Data = "what do ya want for nothing?"
        let key2 = b"Jefe";
        let data2 = b"what do ya want for nothing?";
        let expected2 =
            parse_hex_test("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        let mut mac2 = HmacSha256::new_from_slice(key2).unwrap();
        mac2.update(data2);
        assert_eq!(
            mac2.finalize().into_bytes().as_slice(),
            expected2.as_slice()
        );

        // Test Case 3: Key = [0xaa; 20], Data = [0xdd; 50]
        let key3 = [0xaa; 20];
        let data3 = [0xdd; 50];
        let expected3 =
            parse_hex_test("773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe");
        let mut mac3 = HmacSha256::new_from_slice(&key3).unwrap();
        mac3.update(&data3);
        assert_eq!(
            mac3.finalize().into_bytes().as_slice(),
            expected3.as_slice()
        );

        // Test Case 4: Key = 25 bytes (0x01..=0x19), Data = [0xcd; 50]
        let mut key4 = [0u8; 25];
        for (i, item) in key4.iter_mut().enumerate() {
            *item = (i + 1) as u8;
        }
        let data4 = [0xcd; 50];
        let expected4 =
            parse_hex_test("82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b");
        let mut mac4 = HmacSha256::new_from_slice(&key4).unwrap();
        mac4.update(&data4);
        assert_eq!(
            mac4.finalize().into_bytes().as_slice(),
            expected4.as_slice()
        );

        // Test Case 6: Key = [0xaa; 131], Data = "Test Using Larger Than Block-Size Key - Hash Key First"
        let key6 = [0xaa; 131];
        let data6 = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let expected6 =
            parse_hex_test("60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54");
        let mut mac6 = HmacSha256::new_from_slice(&key6).unwrap();
        mac6.update(data6);
        assert_eq!(
            mac6.finalize().into_bytes().as_slice(),
            expected6.as_slice()
        );
    }

    #[test]
    fn hmac_voucher_known_answer_and_field_tampering_rejection() {
        let authority_secret = b"owner-root-secret-for-rfc-and-tampering-test";
        let clock = Arc::new(MockClock::new(10_000));
        let scope = OwnerAuthorizationScope {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity-token".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-golden-001".to_owned(),
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, clock.clone());
        let (valid_req, voucher) = issuer.issue_for_scope(&scope, 1000, "nonce-golden-001");
        assert_eq!(voucher.len(), 32);

        // Verify constant-time comparison helper
        assert!(issuer.verify_voucher_constant_time(&valid_req.owner_material, &voucher));
        let mut bad_voucher = voucher.clone();
        bad_voucher[0] ^= 0x01;
        assert!(!issuer.verify_voucher_constant_time(&bad_voucher, &voucher));

        let verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope.clone(),
            clock.clone(),
        );

        // 1. Untampered request with valid voucher MUST verify and mint TrustedLive
        let gate = verifier.verify(&valid_req).unwrap();
        assert!(gate.is_trusted_live());
        assert_eq!(gate.bound_service_name(), "fixture.scm.a");
        assert_eq!(gate.bound_image_token(), "fixture-image");
        assert_eq!(
            gate.bound_execution_identity_token(),
            "fixture-identity-token"
        );
        assert_eq!(gate.bound_target_host_token(), "fixture-host-token");
        assert_eq!(
            gate.bound_candidate_commit(),
            "028c7efba9ebd3d3925371b356a187eba428041d"
        );
        assert_eq!(gate.bound_transaction_id(), "tx-golden-001");
        assert_eq!(gate.not_before_epoch_seconds(), 10_000);
        assert_eq!(gate.expires_at_epoch_seconds(), 11_000);

        // Tampering tests - each variation MUST be rejected
        let make_verifier = || {
            TokenValidatingOwnerVerifier::with_authority_and_clock(
                authority_secret,
                scope.clone(),
                clock.clone(),
            )
        };

        // 2. Tampered service_name
        let mut tampered = valid_req.clone();
        tampered.service_name = "fixture.scm.b".to_owned();
        assert!(make_verifier().verify(&tampered).is_err());

        // 3. Tampered image_token
        let mut tampered = valid_req.clone();
        tampered.image_token = "tampered-image".to_owned();
        assert!(make_verifier().verify(&tampered).is_err());

        // 4. Tampered execution_identity_token
        let mut tampered = valid_req.clone();
        tampered.execution_identity_token = "tampered-identity".to_owned();
        assert!(make_verifier().verify(&tampered).is_err());

        // 5. Tampered target_host_token
        let mut tampered = valid_req.clone();
        tampered.target_host_token = "tampered-host".to_owned();
        assert!(make_verifier().verify(&tampered).is_err());

        // 6. Tampered candidate_commit
        let mut tampered = valid_req.clone();
        tampered.candidate_commit = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned();
        assert!(make_verifier().verify(&tampered).is_err());

        // 7. Tampered allow_start
        let mut tampered = valid_req.clone();
        tampered.allow_start = false;
        assert!(make_verifier().verify(&tampered).is_err());

        // 8. Tampered allow_stop
        let mut tampered = valid_req.clone();
        tampered.allow_stop = false;
        assert!(make_verifier().verify(&tampered).is_err());

        // 9. Tampered allow_change_config
        let mut tampered = valid_req.clone();
        tampered.allow_change_config = false;
        assert!(make_verifier().verify(&tampered).is_err());

        // 10. Tampered transaction_id
        let mut tampered = valid_req.clone();
        tampered.transaction_id = "tx-tampered-999".to_owned();
        assert!(make_verifier().verify(&tampered).is_err());

        // 11. Tampered not_before
        let mut tampered = valid_req.clone();
        tampered.not_before_epoch_seconds = 9_999;
        assert!(make_verifier().verify(&tampered).is_err());

        // 12. Tampered expires_at
        let mut tampered = valid_req.clone();
        tampered.expires_at_epoch_seconds = 12_000;
        assert!(make_verifier().verify(&tampered).is_err());

        // 13. Tampered nonce
        let mut tampered = valid_req.clone();
        tampered.nonce = "nonce-tampered-xxx".to_owned();
        assert!(make_verifier().verify(&tampered).is_err());

        // 14. Tampered owner_material (1 bit flip)
        let mut tampered = valid_req.clone();
        tampered.owner_material[15] ^= 0x01;
        assert!(make_verifier().verify(&tampered).is_err());

        // 15. Truncated owner_material
        let mut tampered = valid_req.clone();
        tampered.owner_material.truncate(16);
        assert!(make_verifier().verify(&tampered).is_err());

        // 16. Different authority secret
        let wrong_secret_verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            b"completely-wrong-authority-secret-xyz",
            scope,
            clock,
        );
        assert!(wrong_secret_verifier.verify(&valid_req).is_err());
    }

    #[test]
    fn dispatch_post_mint_expiry_refuses_with_zero_backend_writes() {
        let authority_secret = b"owner-root-secret-for-expiry-test";
        let mock_clock = Arc::new(MockClock::new(1000));
        let scope = OwnerAuthorizationScope {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-post-mint-expiry".to_owned(),
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, mock_clock.clone());
        let (request, _) = issuer.issue_for_scope(&scope, 500, "nonce-expiry-test");
        let verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope,
            mock_clock.clone(),
        );
        // Mint valid TrustedLive gate at t = 1000, expires at t = 1500
        let gate = verifier.verify(&request).unwrap();
        assert!(gate.is_trusted_live());

        // Advance clock past expiration to t = 1501
        mock_clock.set_time(1501);

        let mock_backend = MockScmBackend::new();
        mock_backend.insert(test_fixture_record(
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));

        // 1. ScmControlExecutor::execute_stop refused, 0 backend calls
        let exec = ScmControlExecutor::new(
            mock_backend.clone(),
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            gate.clone(),
        )
        .with_clock(mock_clock.clone());

        let prestate = ServicePrestateCapture {
            service_name: "fixture.scm.a".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: true,
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            transaction_id: "tx-post-mint-expiry".to_owned(),
        };
        let restore_plan = capture_restore_plan(&prestate);

        let receipt_stop = exec.execute_stop(Some(restore_plan.clone()));
        assert_eq!(receipt_stop.stage, ScmTransactionStage::Refused);
        assert_eq!(receipt_stop.reason.as_deref(), Some("scm.access-denied"));
        assert_eq!(mock_backend.stop_calls(), 0);

        // 2. ScmControlExecutor::execute_start refused, 0 backend calls
        let receipt_start = exec.execute_start(Some(restore_plan.clone()));
        assert_eq!(receipt_start.stage, ScmTransactionStage::Refused);
        assert_eq!(receipt_start.reason.as_deref(), Some("scm.access-denied"));
        assert_eq!(mock_backend.start_calls(), 0);

        // 3. ScmControlExecutor::restore refused, 0 backend calls
        let mut fake_admission = FakeGithubAdmissionBackend::new_with_selector_present(false);
        let receipt_restore = exec.restore(&restore_plan, &mut fake_admission);
        assert_eq!(receipt_restore.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt_restore.restore_pass);
        assert_eq!(receipt_restore.reason.as_deref(), Some("scm.access-denied"));
        assert_eq!(mock_backend.change_calls(), 0);
        assert_eq!(mock_backend.start_calls(), 0);
        assert_eq!(mock_backend.stop_calls(), 0);

        // 4. PrivilegedScmHelper::dispatch refused for start, stop, and config; 0 backend calls
        let helper = PrivilegedScmHelper::new(mock_backend.clone(), gate).with_clock(mock_clock);

        let req_start = PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.scm.a".to_owned(),
            expected_image_token: "fixture-image".to_owned(),
            expected_execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            transaction_id: "tx-post-mint-expiry".to_owned(),
        };
        let rep_start = helper.dispatch(&req_start);
        assert!(!rep_start.applied);
        assert_eq!(
            rep_start.refusal_reason.as_deref(),
            Some("scm.access-denied")
        );
        assert_eq!(mock_backend.start_calls(), 0);

        let req_stop = PrivilegedScmRequest {
            action: PrivilegedScmAction::StopService,
            ..req_start.clone()
        };
        let rep_stop = helper.dispatch(&req_stop);
        assert!(!rep_stop.applied);
        assert_eq!(
            rep_stop.refusal_reason.as_deref(),
            Some("scm.access-denied")
        );
        assert_eq!(mock_backend.stop_calls(), 0);

        let req_config = PrivilegedScmRequest {
            action: PrivilegedScmAction::ChangeStartMode {
                mode: ServiceStartMode::Disabled,
            },
            ..req_start
        };
        let rep_config = helper.dispatch(&req_config);
        assert!(!rep_config.applied);
        assert_eq!(
            rep_config.refusal_reason.as_deref(),
            Some("scm.access-denied")
        );
        assert_eq!(mock_backend.change_calls(), 0);

        // Total write calls MUST be strictly ZERO
        assert_eq!(mock_backend.stop_calls(), 0);
        assert_eq!(mock_backend.start_calls(), 0);
        assert_eq!(mock_backend.change_calls(), 0);
    }

    #[test]
    fn dispatch_context_drift_refuses_with_zero_backend_writes() {
        let authority_secret = b"owner-root-secret-context-drift-test";
        let mock_clock = Arc::new(MockClock::new(1000));
        let scope = OwnerAuthorizationScope {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-drift-001".to_owned(),
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, mock_clock.clone());
        let (request, _) = issuer.issue_for_scope(&scope, 10_000, "nonce-drift-test");
        let verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope,
            mock_clock.clone(),
        );
        let gate = verifier.verify(&request).unwrap();
        assert!(gate.is_trusted_live());

        let mock_backend = MockScmBackend::new();
        mock_backend.insert(test_fixture_record(
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));

        // 1. Host drift in helper
        let helper_drift_host = PrivilegedScmHelper::new(mock_backend.clone(), gate.clone())
            .with_clock(mock_clock.clone());
        let req_drift_host = PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.scm.a".to_owned(),
            expected_image_token: "fixture-image".to_owned(),
            expected_execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "wrong-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            transaction_id: "tx-drift-001".to_owned(),
        };
        let rep_host = helper_drift_host.dispatch(&req_drift_host);
        assert!(!rep_host.applied);
        assert_eq!(
            rep_host.refusal_reason.as_deref(),
            Some("scm.identity-mismatch")
        );
        assert_eq!(mock_backend.start_calls(), 0);

        // 2. Candidate commit drift in helper
        let req_drift_commit = PrivilegedScmRequest {
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned(),
            ..req_drift_host.clone()
        };
        let rep_commit = helper_drift_host.dispatch(&req_drift_commit);
        assert!(!rep_commit.applied);
        assert_eq!(
            rep_commit.refusal_reason.as_deref(),
            Some("scm.identity-mismatch")
        );
        assert_eq!(mock_backend.start_calls(), 0);

        // 3. Transaction ID drift in helper
        let req_drift_tx = PrivilegedScmRequest {
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            transaction_id: "tx-drift-wrong".to_owned(),
            ..req_drift_host.clone()
        };
        let rep_tx = helper_drift_host.dispatch(&req_drift_tx);
        assert!(!rep_tx.applied);
        assert_eq!(
            rep_tx.refusal_reason.as_deref(),
            Some("scm.identity-mismatch")
        );
        assert_eq!(mock_backend.start_calls(), 0);

        // 4. Execution identity drift in executor
        let exec_drift_id = ScmControlExecutor::new(
            mock_backend.clone(),
            "fixture.scm.a",
            "fixture-image",
            "wrong-identity",
            gate.clone(),
        )
        .with_clock(mock_clock.clone());

        let prestate = ServicePrestateCapture {
            service_name: "fixture.scm.a".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: true,
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "wrong-identity".to_owned(),
            transaction_id: "tx-drift-001".to_owned(),
        };
        let restore_plan = capture_restore_plan(&prestate);
        let receipt_stop = exec_drift_id.execute_stop(Some(restore_plan));
        assert_eq!(receipt_stop.stage, ScmTransactionStage::Refused);
        assert_eq!(
            receipt_stop.reason.as_deref(),
            Some("scm.identity-mismatch")
        );
        assert_eq!(mock_backend.stop_calls(), 0);

        // Zero write calls on all drift paths
        assert_eq!(mock_backend.stop_calls(), 0);
        assert_eq!(mock_backend.start_calls(), 0);
        assert_eq!(mock_backend.change_calls(), 0);
    }

    #[test]
    fn dispatch_privilege_escalation_refuses_with_zero_backend_writes() {
        let authority_secret = b"owner-root-secret-priv-esc-test";
        let mock_clock = Arc::new(MockClock::new(1000));
        let mock_backend = MockScmBackend::new();
        mock_backend.insert(test_fixture_record(
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            DiscoveredLifecycle::Listening,
            ServiceStartMode::Manual,
            false,
        ));

        // Part 1: Start-only gate refuses StopService & ChangeStartMode
        let start_only_scope = OwnerAuthorizationScope {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: false,
            allow_change_config: false,
            transaction_id: "tx-start-only".to_owned(),
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, mock_clock.clone());
        let (req_start_only, _) =
            issuer.issue_for_scope(&start_only_scope, 5000, "nonce-start-only");
        let verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            start_only_scope,
            mock_clock.clone(),
        );
        let start_only_gate = verifier.verify(&req_start_only).unwrap();

        let helper_start_only =
            PrivilegedScmHelper::new(mock_backend.clone(), start_only_gate.clone())
                .with_clock(mock_clock.clone());
        let req_escalate_stop = PrivilegedScmRequest {
            action: PrivilegedScmAction::StopService,
            service_name: "fixture.scm.a".to_owned(),
            expected_image_token: "fixture-image".to_owned(),
            expected_execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            transaction_id: "tx-start-only".to_owned(),
        };
        let rep_esc_stop = helper_start_only.dispatch(&req_escalate_stop);
        assert!(!rep_esc_stop.applied);
        assert_eq!(
            rep_esc_stop.refusal_reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(mock_backend.stop_calls(), 0);

        let req_escalate_config = PrivilegedScmRequest {
            action: PrivilegedScmAction::ChangeStartMode {
                mode: ServiceStartMode::Disabled,
            },
            ..req_escalate_stop
        };
        let rep_esc_cfg = helper_start_only.dispatch(&req_escalate_config);
        assert!(!rep_esc_cfg.applied);
        assert_eq!(
            rep_esc_cfg.refusal_reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(mock_backend.change_calls(), 0);

        let exec_start_only = ScmControlExecutor::new(
            mock_backend.clone(),
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            start_only_gate,
        )
        .with_clock(mock_clock.clone());

        let prestate = ServicePrestateCapture {
            service_name: "fixture.scm.a".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Manual,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: true,
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            transaction_id: "tx-start-only".to_owned(),
        };
        let restore_plan = capture_restore_plan(&prestate);
        let receipt_stop = exec_start_only.execute_stop(Some(restore_plan));
        assert_eq!(receipt_stop.stage, ScmTransactionStage::Refused);
        assert_eq!(receipt_stop.reason.as_deref(), Some("scm.live-gate-closed"));
        assert_eq!(mock_backend.stop_calls(), 0);

        // Part 2: Stop-only gate refuses execute_start & helper StartService
        let stop_only_scope = OwnerAuthorizationScope {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: false,
            allow_stop: true,
            allow_change_config: false,
            transaction_id: "tx-stop-only".to_owned(),
        };
        let (req_stop_only, _) = issuer.issue_for_scope(&stop_only_scope, 5000, "nonce-stop-only");
        let verifier_stop = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            stop_only_scope,
            mock_clock.clone(),
        );
        let stop_only_gate = verifier_stop.verify(&req_stop_only).unwrap();

        let helper_stop_only =
            PrivilegedScmHelper::new(mock_backend.clone(), stop_only_gate.clone())
                .with_clock(mock_clock.clone());
        let req_escalate_start = PrivilegedScmRequest {
            action: PrivilegedScmAction::StartService,
            service_name: "fixture.scm.a".to_owned(),
            expected_image_token: "fixture-image".to_owned(),
            expected_execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            transaction_id: "tx-stop-only".to_owned(),
        };
        let rep_esc_start = helper_stop_only.dispatch(&req_escalate_start);
        assert!(!rep_esc_start.applied);
        assert_eq!(
            rep_esc_start.refusal_reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(mock_backend.start_calls(), 0);

        let exec_stop_only = ScmControlExecutor::new(
            mock_backend.clone(),
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            stop_only_gate,
        )
        .with_clock(mock_clock);
        let receipt_start = exec_stop_only.execute_start(None);
        assert_eq!(receipt_start.stage, ScmTransactionStage::Refused);
        assert_eq!(
            receipt_start.reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(mock_backend.start_calls(), 0);

        // Zero backend write calls across all privilege escalation attempts
        assert_eq!(mock_backend.stop_calls(), 0);
        assert_eq!(mock_backend.start_calls(), 0);
        assert_eq!(mock_backend.change_calls(), 0);
    }

    #[test]
    fn restore_path_authorization_rules_explicit() {
        let mock_backend = MockScmBackend::new();
        mock_backend.insert(test_fixture_record(
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            DiscoveredLifecycle::Stopped,
            ServiceStartMode::Manual,
            false,
        ));

        let prestate = ServicePrestateCapture {
            service_name: "fixture.scm.a".to_owned(),
            state: DiscoveredLifecycle::Listening,
            start_mode: ServiceStartMode::Automatic,
            image_path_matches: true,
            ownership_verified: true,
            worker_busy: false,
            admission_selector_absent: true,
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            transaction_id: "tx-restore-rules".to_owned(),
        };
        let mut restore_plan = capture_restore_plan(&prestate);
        let mut fake_admission = FakeGithubAdmissionBackend::new_with_selector_present(false);

        // Rule 1: plan.requires_owner_gate == true with closed gate -> fails closed, 0 writes
        restore_plan.requires_owner_gate = true;
        let exec_closed = ScmControlExecutor::new(
            mock_backend.clone(),
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            LiveScmExecutionGate::CLOSED,
        );
        let receipt_closed = exec_closed.restore(&restore_plan, &mut fake_admission);
        assert_eq!(receipt_closed.stage, ScmTransactionStage::RestoreFailed);
        assert!(!receipt_closed.restore_pass);
        assert_eq!(
            receipt_closed.reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(mock_backend.change_calls(), 0);
        assert_eq!(mock_backend.start_calls(), 0);

        // Rule 2: plan.requires_owner_gate == false still fails closed on LiveScmExecutionGate::CLOSED
        // Non-gated restore is permitted ONLY under explicit SyntheticTestOnly gates, never CLOSED
        restore_plan.requires_owner_gate = false;
        let receipt_closed_bypass_attempt = exec_closed.restore(&restore_plan, &mut fake_admission);
        assert_eq!(
            receipt_closed_bypass_attempt.stage,
            ScmTransactionStage::RestoreFailed
        );
        assert!(!receipt_closed_bypass_attempt.restore_pass);
        assert_eq!(
            receipt_closed_bypass_attempt.reason.as_deref(),
            Some("scm.live-gate-closed")
        );
        assert_eq!(mock_backend.change_calls(), 0);
        assert_eq!(mock_backend.start_calls(), 0);

        let exec_synthetic = ScmControlExecutor::new(
            mock_backend.clone(),
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            open_synthetic_gate(),
        );
        let receipt_synthetic = exec_synthetic.restore(&restore_plan, &mut fake_admission);
        assert_eq!(receipt_synthetic.stage, ScmTransactionStage::Completed);
        assert!(receipt_synthetic.restore_pass);
        assert_eq!(mock_backend.change_calls(), 1);
        assert_eq!(mock_backend.start_calls(), 1);

        // Rule 3: plan.requires_owner_gate == true with fully valid unexpired TrustedLive gate -> succeeds
        restore_plan.requires_owner_gate = true;
        let authority_secret = b"owner-root-secret-restore-test";
        let mock_clock = Arc::new(MockClock::new(1000));
        let scope = OwnerAuthorizationScope {
            service_name: "fixture.scm.a".to_owned(),
            image_token: "fixture-image".to_owned(),
            execution_identity_token: "fixture-identity".to_owned(),
            target_host_token: "fixture-host-token".to_owned(),
            candidate_commit: "028c7efba9ebd3d3925371b356a187eba428041d".to_owned(),
            allow_start: true,
            allow_stop: true,
            allow_change_config: true,
            transaction_id: "tx-restore-rules".to_owned(),
        };
        let issuer = OwnerAuthorizationIssuer::with_clock(authority_secret, mock_clock.clone());
        let (req, _) = issuer.issue_for_scope(&scope, 10_000, "nonce-restore-valid");
        let verifier = TokenValidatingOwnerVerifier::with_authority_and_clock(
            authority_secret,
            scope,
            mock_clock.clone(),
        );
        let live_gate = verifier.verify(&req).unwrap();

        let exec_live = ScmControlExecutor::new(
            mock_backend.clone(),
            "fixture.scm.a",
            "fixture-image",
            "fixture-identity",
            live_gate,
        )
        .with_clock(mock_clock);

        let receipt_live = exec_live.restore(&restore_plan, &mut fake_admission);
        assert_eq!(receipt_live.stage, ScmTransactionStage::Completed);
        assert!(receipt_live.restore_pass);
    }

    fn mock_backend_c1() -> MockScmBackend {
        let backend = MockScmBackend::new();
        backend.insert(MockServiceRecord {
            observation: ServiceObservation {
                service_name: "fixture.audit.a".into(),
                state: DiscoveredLifecycle::Listening,
                start_mode: ServiceStartMode::Manual,
                image_path_matches: true,
                ownership_verified: true,
            },
            worker_busy: false,
            image_token: "fixture-image".into(),
            execution_identity_token: "fixture-identity".into(),
        });
        backend
    }

    fn live_gate_c1() -> LiveScmExecutionGate {
        LiveScmExecutionGate::mint_trusted_live(
            "fixture.audit.a",
            "fixture-image",
            "fixture-identity",
            "host-a",
            "candidate-a",
            "tx-a",
            false,
            true,
            false,
            1000,
            1100,
        )
        .unwrap()
    }

    fn request_c1() -> PrivilegedScmRequest {
        PrivilegedScmRequest {
            action: PrivilegedScmAction::StopService,
            service_name: "fixture.audit.a".into(),
            expected_image_token: "fixture-image".into(),
            expected_execution_identity_token: "fixture-identity".into(),
            target_host_token: "host-a".into(),
            candidate_commit: "candidate-a".into(),
            transaction_id: "tx-a".into(),
        }
    }

    struct ExpiringBackend {
        backend: MockScmBackend,
        clock: Arc<MockClock>,
    }

    impl ScmControlBackend for ExpiringBackend {
        fn query_exact(&self, name: &str) -> Result<ScmExactSnapshot, ScmApiError> {
            self.clock.set_time(1200);
            self.backend.query_exact(name)
        }
        fn start(&self, name: &str) -> Result<(), ScmApiError> {
            self.backend.start(name)
        }
        fn stop(&self, name: &str) -> Result<(), ScmApiError> {
            self.backend.stop(name)
        }
        fn change_start_mode(&self, name: &str, mode: ServiceStartMode) -> Result<(), ScmApiError> {
            self.backend.change_start_mode(name, mode)
        }
    }

    struct AbsentAdmission;
    impl AdmissionSelectorObserver for AbsentAdmission {
        fn observe_selector_absent(&mut self) -> Result<bool, ScmApiError> {
            Ok(true)
        }
    }

    #[test]
    fn regression_c1_request_cannot_override_executor_host_and_candidate() {
        let backend = mock_backend_c1();
        let helper = PrivilegedScmHelper::new(backend.clone(), live_gate_c1())
            .with_clock(Arc::new(MockClock::new(1000)))
            .with_host_and_commit("host-b", "candidate-b");
        let result = helper.dispatch(&request_c1());
        assert!(
            !result.applied,
            "request must not override executor host/candidate context"
        );
        assert_eq!(
            backend.stop_calls(),
            0,
            "must produce zero write calls on context mismatch"
        );
    }

    #[test]
    fn regression_c1_expiry_during_readback_refuses_with_zero_writes() {
        let backend = mock_backend_c1();
        let clock = Arc::new(MockClock::new(1000));
        let helper = PrivilegedScmHelper::new(
            ExpiringBackend {
                backend: backend.clone(),
                clock: clock.clone(),
            },
            live_gate_c1(),
        )
        .with_clock(clock.clone());
        let result = helper.dispatch(&request_c1());
        assert_eq!(clock.now_epoch_seconds(), 1200);
        assert!(
            !result.applied,
            "mutation must be refused if gate expired during query/readback"
        );
        assert_eq!(
            backend.stop_calls(),
            0,
            "must produce zero write calls after expiry"
        );
    }

    #[test]
    fn regression_c1_restore_flag_false_cannot_bypass_closed_gate() {
        let backend = mock_backend_c1();
        let executor = ScmControlExecutor::new(
            backend.clone(),
            "fixture.audit.a",
            "fixture-image",
            "fixture-identity",
            LiveScmExecutionGate::CLOSED,
        );
        let mut plan = capture_restore_plan(&executor.capture_prestate(false, true).unwrap());
        plan.requires_owner_gate = false;
        let result = executor.restore(&plan, &mut AbsentAdmission);
        assert_eq!(
            result.stage,
            ScmTransactionStage::RestoreFailed,
            "closed gate must refuse restore"
        );
        assert!(
            !result.restore_pass,
            "restore_pass must be false when gate is closed"
        );
        assert_eq!(
            backend.change_calls(),
            0,
            "must produce zero change_start_mode calls"
        );
        assert_eq!(backend.start_calls(), 0, "must produce zero start calls");
    }

    #[test]
    fn regression_c1_issuer_debug_redacts_authority_secret() {
        let issuer = OwnerAuthorizationIssuer::new(b"super-secret-key-material-never-log");
        let debug_str = format!("{:?}", issuer);
        assert!(!debug_str.contains("super-secret-key-material-never-log"));
        assert!(debug_str.contains("<redacted>"));
    }

    #[test]
    fn regression_c2_image_path_token_unified_across_quoted_and_args() {
        let bare = r"C:\runners\a\bin\RunnerService.exe";
        let quoted = r#""C:\runners\a\bin\RunnerService.exe""#;
        let with_args = r#""C:\runners\a\bin\RunnerService.exe" --run"#;
        let unquoted_args = r"C:\runners\a\bin\RunnerService.exe -service";

        let token_bare = opaque_image_token(bare);
        let token_quoted = opaque_image_token(quoted);
        let token_with_args = opaque_image_token(with_args);
        let token_unquoted_args = opaque_image_token(unquoted_args);

        assert_eq!(token_bare, token_quoted);
        assert_eq!(token_bare, token_with_args);
        assert_eq!(token_bare, token_unquoted_args);
    }
}
