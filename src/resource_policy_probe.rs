//! Read-only capability probe for future v0.2-B Windows resource policy.
//!
//! Detects whether Job Object style containment APIs are available to link
//! against on this host. Never creates Job Objects, never attaches processes,
//! and never mutates Runner Services or Worker trees.

use serde::{Deserialize, Serialize};

/// Host capability snapshot for resource-policy planning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcePolicyCapability {
    pub job_objects_linkable: bool,
    pub enforcement_authorized: bool,
    pub notes: Vec<&'static str>,
}

/// Probes compile/host support without performing enforcement.
pub fn probe_resource_policy_capabilities() -> ResourcePolicyCapability {
    #[cfg(windows)]
    {
        ResourcePolicyCapability {
            // windows-sys Job Object symbols are available when Win32_System_JobObjects
            // is enabled; v0.1 Cargo.toml may not yet enable that feature. Report the
            // planning truth without enabling enforcement.
            job_objects_linkable: cfg!(feature = "windows-job-objects"),
            enforcement_authorized: false,
            notes: vec![
                "SOURCE_AND_SANDBOX_ONLY",
                "enforcement requires separate v0.2-B Owner Goal",
                "probe does not create or attach Job Objects",
            ],
        }
    }
    #[cfg(not(windows))]
    {
        ResourcePolicyCapability {
            job_objects_linkable: false,
            enforcement_authorized: false,
            notes: vec![
                "non-windows host",
                "Windows Job Object enforcement is out of scope here",
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_never_authorizes_enforcement() {
        let cap = probe_resource_policy_capabilities();
        assert!(!cap.enforcement_authorized);
        assert!(!cap.notes.is_empty());
    }
}
