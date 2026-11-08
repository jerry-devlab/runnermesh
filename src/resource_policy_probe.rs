//! Read-only capability probe for future v0.2-B Windows resource policy.
//!
//! Detects host planning signals for Job Object style containment. Never
//! creates Job Objects, never attaches processes, and never mutates Runner
//! Services or Worker trees.

use serde::{Deserialize, Serialize};

/// Host capability snapshot for resource-policy planning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcePolicyCapability {
    pub platform_is_windows: bool,
    pub job_object_probe_implemented: bool,
    pub enforcement_authorized: bool,
    pub notes: Vec<String>,
}

/// Probes planning support without performing enforcement.
pub fn probe_resource_policy_capabilities() -> ResourcePolicyCapability {
    #[cfg(windows)]
    {
        ResourcePolicyCapability {
            platform_is_windows: true,
            // Containment APIs exist on Windows hosts, but this Goal only
            // records a read-only planning probe. Linking/enforcement stays
            // behind a future Owner-authorized v0.2-B Goal.
            job_object_probe_implemented: true,
            enforcement_authorized: false,
            notes: vec![
                "SOURCE_AND_SANDBOX_ONLY".to_owned(),
                "enforcement requires separate v0.2-B Owner Goal".to_owned(),
                "probe does not create or attach Job Objects".to_owned(),
            ],
        }
    }
    #[cfg(not(windows))]
    {
        ResourcePolicyCapability {
            platform_is_windows: false,
            job_object_probe_implemented: false,
            enforcement_authorized: false,
            notes: vec![
                "non-windows host".to_owned(),
                "Windows Job Object enforcement is out of scope here".to_owned(),
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
        #[cfg(windows)]
        assert!(cap.platform_is_windows && cap.job_object_probe_implemented);
        #[cfg(not(windows))]
        assert!(!cap.platform_is_windows && !cap.job_object_probe_implemented);
    }
}
