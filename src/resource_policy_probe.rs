//! Read-only capability probe for future v0.2-B Windows resource policy (P08-02).
//!
//! Detects host planning signals for Job Object style containment and EcoQoS.
//! Never creates Job Objects, never attaches processes, never alters CPU affinity
//! or priority classes, and never mutates Runner Services or Worker trees.

use serde::{Deserialize, Serialize};

/// Host capability snapshot for resource-policy planning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcePolicyCapability {
    pub platform_is_windows: bool,
    pub job_object_probe_implemented: bool,
    pub nested_job_objects_supported: bool,
    pub eco_qos_supported: bool,
    pub enforcement_authorized: bool,
    pub query_only: bool,
    pub notes: Vec<String>,
}

#[cfg(windows)]
fn probe_windows_os_version() -> (u32, u32, u32) {
    #[repr(C)]
    struct OsVersionInfoExW {
        dw_os_version_info_size: u32,
        dw_major_version: u32,
        dw_minor_version: u32,
        dw_build_number: u32,
        dw_platform_id: u32,
        sz_csd_version: [u16; 128],
        w_service_pack_major: u16,
        w_service_pack_minor: u16,
        w_suite_mask: u16,
        w_product_type: u8,
        w_reserved: u8,
    }
    type RtlGetVersionFn = unsafe extern "system" fn(*mut OsVersionInfoExW) -> i32;

    unsafe {
        let ntdll = windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(
            c"ntdll.dll".as_ptr().cast(),
        );
        if !ntdll.is_null() {
            let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
                ntdll,
                c"RtlGetVersion".as_ptr().cast(),
            );
            if let Some(proc) = proc {
                let rtl_get_version: RtlGetVersionFn = std::mem::transmute(proc);
                let mut info: OsVersionInfoExW = std::mem::zeroed();
                info.dw_os_version_info_size = std::mem::size_of::<OsVersionInfoExW>() as u32;
                if rtl_get_version(&mut info) == 0 {
                    return (
                        info.dw_major_version,
                        info.dw_minor_version,
                        info.dw_build_number,
                    );
                }
            }
        }
    }
    (0, 0, 0)
}

/// Probes planning support without performing enforcement.
pub fn probe_resource_policy_capabilities() -> ResourcePolicyCapability {
    #[cfg(windows)]
    {
        let (major, minor, build) = probe_windows_os_version();
        // Windows 8 / Server 2012+ (NT 6.2+, build >= 9200) supports nested Job Objects.
        let nested_jobs = major > 6 || (major == 6 && minor >= 2) || build >= 9200;
        // Windows 10 Build 19041+ (Version 2004+) or Windows 11 supports EcoQoS.
        let eco_qos = build >= 19041 || major >= 11;
        ResourcePolicyCapability {
            platform_is_windows: true,
            job_object_probe_implemented: true,
            nested_job_objects_supported: nested_jobs,
            eco_qos_supported: eco_qos,
            enforcement_authorized: false,
            query_only: true,
            notes: vec![
                "SOURCE_AND_SANDBOX_ONLY".to_owned(),
                "read-only capability detection only; zero mutation".to_owned(),
                "probe does not create Job Objects or attach processes".to_owned(),
                "enforcement and containment reserved for future v0.2 programs".to_owned(),
                format!("detected_windows_build={build}"),
            ],
        }
    }
    #[cfg(not(windows))]
    {
        ResourcePolicyCapability {
            platform_is_windows: false,
            job_object_probe_implemented: false,
            nested_job_objects_supported: false,
            eco_qos_supported: false,
            enforcement_authorized: false,
            query_only: true,
            notes: vec![
                "non-windows host".to_owned(),
                "Windows Job Object and EcoQoS capabilities are out of scope here".to_owned(),
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
        assert!(cap.query_only);
        assert!(!cap.notes.is_empty());
        #[cfg(windows)]
        {
            assert!(cap.platform_is_windows && cap.job_object_probe_implemented);
            assert!(cap.nested_job_objects_supported);
        }
        #[cfg(not(windows))]
        {
            assert!(!cap.platform_is_windows && !cap.job_object_probe_implemented);
            assert!(!cap.nested_job_objects_supported);
            assert!(!cap.eco_qos_supported);
        }
    }
}
