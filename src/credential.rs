use crate::admission::secure_zero_bytes;
use crate::{AdmissionBackendError, CredentialLease, CredentialProvider, CredentialReference};

pub const WINDOWS_CREDENTIAL_MANAGER_PROVIDER: &str = "windows-credential-manager";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialStoreError {
    NotFound,
    Unavailable,
}

/// Provider-neutral read-only boundary for an OS-backed credential store.
/// Implementations return owned bytes so the lease can zero them in place.
pub trait CredentialStore {
    fn read_generic(&mut self, key: &str) -> Result<Vec<u8>, CredentialStoreError>;
}

/// Resolves an opaque normal-configuration reference into a short-lived,
/// redacted credential lease. This type deliberately implements neither
/// `Serialize` nor `Debug`.
pub struct CredentialProviderAdapter<S> {
    provider_name: &'static str,
    store: S,
}

impl<S> CredentialProviderAdapter<S> {
    pub fn new(provider_name: &'static str, store: S) -> Self {
        Self {
            provider_name,
            store,
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }
}

impl<S: CredentialStore> CredentialProvider for CredentialProviderAdapter<S> {
    fn resolve(
        &mut self,
        reference: &CredentialReference,
    ) -> Result<CredentialLease, AdmissionBackendError> {
        if !reference.is_valid() || reference.provider != self.provider_name {
            return Err(AdmissionBackendError::CredentialResolutionFailed);
        }
        let secret = self
            .store
            .read_generic(&reference.key)
            .map_err(|error| match error {
                CredentialStoreError::NotFound => AdmissionBackendError::CredentialUnavailable,
                CredentialStoreError::Unavailable => {
                    AdmissionBackendError::CredentialResolutionFailed
                }
            })?;
        CredentialLease::from_owned_secret(secret)
            .map_err(|_| AdmissionBackendError::CredentialMalformed)
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsCredentialManagerStore;

#[cfg(windows)]
impl CredentialStore for WindowsCredentialManagerStore {
    fn read_generic(&mut self, key: &str) -> Result<Vec<u8>, CredentialStoreError> {
        use std::{ffi::c_void, ptr, slice};

        use windows_sys::Win32::{
            Foundation::{GetLastError, ERROR_NOT_FOUND, ERROR_NO_SUCH_LOGON_SESSION},
            Security::Credentials::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC},
        };

        struct CredentialBuffer(*mut CREDENTIALW);

        impl Drop for CredentialBuffer {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    // SAFETY: CredReadW owns this single allocation and requires
                    // CredFree exactly once.
                    unsafe { CredFree(self.0.cast::<c_void>()) };
                }
            }
        }

        if CredentialReference::new(WINDOWS_CREDENTIAL_MANAGER_PROVIDER, key).is_err() {
            return Err(CredentialStoreError::Unavailable);
        }
        let target = wide_nul(key);
        let mut raw = ptr::null_mut::<CREDENTIALW>();
        // SAFETY: target is NUL terminated and raw is a valid out pointer.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 {
            // SAFETY: GetLastError is read immediately after the failed call.
            return match unsafe { GetLastError() } {
                ERROR_NOT_FOUND => Err(CredentialStoreError::NotFound),
                ERROR_NO_SUCH_LOGON_SESSION => Err(CredentialStoreError::Unavailable),
                _ => Err(CredentialStoreError::Unavailable),
            };
        }
        let buffer = CredentialBuffer(raw);
        if raw.is_null() {
            return Err(CredentialStoreError::Unavailable);
        }
        // SAFETY: a successful CredReadW returns a valid CREDENTIALW allocation
        // held by buffer for this scope.
        let credential = unsafe { &mut *raw };
        let size = credential.CredentialBlobSize as usize;
        if size == 0 || credential.CredentialBlob.is_null() {
            return Ok(Vec::new());
        }
        // SAFETY: CredentialBlob points to size writable bytes within the
        // CredReadW allocation. Copy first, then clear the transient OS buffer.
        let blob = unsafe { slice::from_raw_parts_mut(credential.CredentialBlob, size) };
        let secret = blob.to_vec();
        secure_zero_bytes(blob);
        drop(buffer);
        Ok(secret)
    }
}

#[cfg(windows)]
pub struct WindowsCredentialManagerProvider {
    inner: CredentialProviderAdapter<WindowsCredentialManagerStore>,
}

#[cfg(windows)]
impl WindowsCredentialManagerProvider {
    pub fn new() -> Self {
        Self {
            inner: CredentialProviderAdapter::new(
                WINDOWS_CREDENTIAL_MANAGER_PROVIDER,
                WindowsCredentialManagerStore,
            ),
        }
    }
}

#[cfg(windows)]
impl Default for WindowsCredentialManagerProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl CredentialProvider for WindowsCredentialManagerProvider {
    fn resolve(
        &mut self,
        reference: &CredentialReference,
    ) -> Result<CredentialLease, AdmissionBackendError> {
        self.inner.resolve(reference)
    }
}

#[cfg(windows)]
fn wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeStore(Result<Vec<u8>, CredentialStoreError>);

    impl CredentialStore for FakeStore {
        fn read_generic(&mut self, _key: &str) -> Result<Vec<u8>, CredentialStoreError> {
            self.0.clone()
        }
    }

    fn reference() -> CredentialReference {
        CredentialReference::new(WINDOWS_CREDENTIAL_MANAGER_PROVIDER, "synthetic-h1").unwrap()
    }

    #[test]
    fn missing_and_resolution_failure_are_distinct_and_never_create_a_lease() {
        for (store_error, expected) in [
            (
                CredentialStoreError::NotFound,
                AdmissionBackendError::CredentialUnavailable,
            ),
            (
                CredentialStoreError::Unavailable,
                AdmissionBackendError::CredentialResolutionFailed,
            ),
        ] {
            let mut provider = CredentialProviderAdapter::new(
                WINDOWS_CREDENTIAL_MANAGER_PROVIDER,
                FakeStore(Err(store_error)),
            );
            assert_eq!(provider.resolve(&reference()).unwrap_err(), expected);
        }
    }

    #[test]
    fn malformed_material_is_refused_and_valid_lease_debug_is_redacted() {
        let mut malformed = CredentialProviderAdapter::new(
            WINDOWS_CREDENTIAL_MANAGER_PROVIDER,
            FakeStore(Ok(b"not a bearer token".to_vec())),
        );
        assert_eq!(
            malformed.resolve(&reference()).unwrap_err(),
            AdmissionBackendError::CredentialMalformed
        );

        let mut valid = CredentialProviderAdapter::new(
            WINDOWS_CREDENTIAL_MANAGER_PROVIDER,
            FakeStore(Ok(b"synthetic-token-shape".to_vec())),
        );
        let lease = valid.resolve(&reference()).unwrap();
        assert_eq!(format!("{lease:?}"), "CredentialLease([REDACTED])");
    }

    #[test]
    fn provider_mismatch_fails_without_reading_the_store() {
        let mut provider = CredentialProviderAdapter::new(
            WINDOWS_CREDENTIAL_MANAGER_PROVIDER,
            FakeStore(Ok(b"synthetic-token-shape".to_vec())),
        );
        let other = CredentialReference::new("other-provider", "synthetic-h1").unwrap();
        assert_eq!(
            provider.resolve(&other).unwrap_err(),
            AdmissionBackendError::CredentialResolutionFailed
        );
    }

    #[test]
    fn invalid_deserialized_reference_fails_before_store_resolution() {
        let invalid: CredentialReference = serde_json::from_str(
            r#"{"provider":"windows-credential-manager","key":"truncated\u0000target"}"#,
        )
        .unwrap();
        let mut provider = CredentialProviderAdapter::new(
            WINDOWS_CREDENTIAL_MANAGER_PROVIDER,
            FakeStore(Ok(b"synthetic-token-shape".to_vec())),
        );

        assert!(!invalid.is_valid());
        assert_eq!(
            provider.resolve(&invalid).unwrap_err(),
            AdmissionBackendError::CredentialResolutionFailed
        );
    }

    #[test]
    fn normal_json_serializes_only_the_reference() {
        let reference = reference();
        let json = serde_json::to_string(&reference).unwrap();
        assert!(json.contains(WINDOWS_CREDENTIAL_MANAGER_PROVIDER));
        assert!(json.contains("synthetic-h1"));
        assert!(!json.contains("synthetic-token-shape"));
    }
}
