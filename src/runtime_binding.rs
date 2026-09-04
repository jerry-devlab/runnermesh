//! Non-secret installed-runtime binding for one accepted official runner.
//!
//! The binding lives under the caller-selected installation `config/`
//! directory, outside immutable version slots. It contains only exact local
//! paths, GitHub runner identity, reserved-label ownership, and an opaque
//! credential reference. Credential material is never serialized here.

use serde::{Deserialize, Serialize};

use crate::{AdmissionBinding, ExactLocalRunnerBinding};

pub const INSTALLED_RUNTIME_BINDING_SCHEMA_VERSION: u32 = 1;
/// Installed runtimes bind the owning Windows identity without persisting SID
/// text. The key is the lowercase SHA-256 of the binary SID representation.
pub const WINDOWS_SID_SHA256_IDENTITY_PROVIDER: &str = "windows-sid-sha256-v1";

/// Produces the private opaque reference used to bind an installed runtime to
/// the current Windows user. SID text never leaves the OS observation helper.
#[cfg(windows)]
pub fn current_windows_identity_reference() -> Result<crate::OpaqueIdentityReference, &'static str>
{
    let digest = crate::process_snapshot::current_user_identity_sha256()
        .map_err(|_| "current Windows identity is unavailable")?;
    crate::OpaqueIdentityReference::new(WINDOWS_SID_SHA256_IDENTITY_PROVIDER, digest)
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledRuntimeBinding {
    pub schema_version: u32,
    pub admission: AdmissionBinding,
    pub local: ExactLocalRunnerBinding,
    pub process_probe_executables: Vec<String>,
}

impl InstalledRuntimeBinding {
    pub fn new(
        admission: AdmissionBinding,
        local: ExactLocalRunnerBinding,
        process_probe_executables: Vec<String>,
    ) -> Result<Self, &'static str> {
        let binding = Self {
            schema_version: INSTALLED_RUNTIME_BINDING_SCHEMA_VERSION,
            admission,
            local,
            process_probe_executables,
        };
        if binding.is_valid() {
            Ok(binding)
        } else {
            Err("installed runtime binding is invalid")
        }
    }

    pub fn is_valid(&self) -> bool {
        self.schema_version == INSTALLED_RUNTIME_BINDING_SCHEMA_VERSION
            && self.admission.is_valid()
            && self.admission.has_valid_ownership()
            && self.local.is_valid()
            && self.local.execution_identity_ref.provider == WINDOWS_SID_SHA256_IDENTITY_PROVIDER
            && is_lower_hex_digest(&self.local.execution_identity_ref.key)
            && self.process_probe_executables.len() <= 64
            && self
                .process_probe_executables
                .iter()
                .all(|name| valid_executable_name(name))
            && unique_case_insensitive(&self.process_probe_executables)
    }
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_executable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 260
        && !name.contains(['\0', '\r', '\n', '/', '\\'])
        && !matches!(name, "." | "..")
        && !name.ends_with(['.', ' '])
}

fn unique_case_insensitive(names: &[String]) -> bool {
    for (index, name) in names.iter().enumerate() {
        if names[..index]
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        CredentialReference, OpaqueIdentityReference, RegistrationScope, ReservedLabelOwnership,
    };

    use super::*;

    fn binding() -> InstalledRuntimeBinding {
        let scope = RegistrationScope::Repository {
            owner: "fixture-owner".to_owned(),
            repository: "fixture-repository".to_owned(),
        };
        InstalledRuntimeBinding::new(
            AdmissionBinding::new(
                scope.clone(),
                42,
                "fixture-runner",
                CredentialReference::new("windows-credential-manager", "fixture-reference")
                    .unwrap(),
                Some(ReservedLabelOwnership::for_runner(scope, 42)),
            )
            .unwrap(),
            ExactLocalRunnerBinding::new(
                PathBuf::from(r"C:\fixture\runner"),
                PathBuf::from(r"C:\fixture\work"),
                OpaqueIdentityReference::new(
                    WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .unwrap(),
            )
            .unwrap(),
            vec!["fixture.exe".to_owned()],
        )
        .unwrap()
    }

    #[test]
    fn binding_is_schema_valid_and_contains_no_credential_material() {
        let binding = binding();
        assert!(binding.is_valid());
        let encoded = serde_json::to_string(&binding).unwrap();
        assert!(encoded.contains("fixture-reference"));
        assert!(!encoded.contains("bearer"));
        assert!(serde_json::from_str::<InstalledRuntimeBinding>(&encoded).unwrap() == binding);
    }

    #[test]
    fn invalid_or_duplicate_process_names_fail_closed() {
        for names in [
            vec!["folder\\probe.exe".to_owned()],
            vec!["probe.exe".to_owned(), "PROBE.EXE".to_owned()],
            vec!["probe.exe ".to_owned()],
        ] {
            let mut candidate = binding();
            candidate.process_probe_executables = names;
            assert!(!candidate.is_valid());
        }

        let mut wrong_identity_provider = binding();
        wrong_identity_provider
            .local
            .execution_identity_ref
            .provider = "windows-user".to_owned();
        assert!(!wrong_identity_provider.is_valid());

        let mut malformed_identity_digest = binding();
        malformed_identity_digest.local.execution_identity_ref.key = "not-a-digest".to_owned();
        assert!(!malformed_identity_digest.is_valid());
    }
}
