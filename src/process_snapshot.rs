//! Read-only executable-name and exact-image enumeration for local process
//! observation. Neither path ever launches a child process or invokes a shell.

#[cfg(any(windows, test))]
use std::path::PathBuf;

/// Deliberately small failure surface for consumers that must not expose OS
/// error details through stable RunnerMesh contracts.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessSnapshotError {
    Unavailable,
    Failed,
}

/// One image observed in a point-in-time local process snapshot. Missing path
/// evidence never grants exact-runner authority.
#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessImage {
    pub process_id: u32,
    pub executable_name: String,
    pub executable_path: Option<PathBuf>,
    /// Collected from the same stable process handle as `executable_path` so
    /// PID reuse cannot splice path and identity evidence.
    pub user_matches_current: Option<bool>,
}

/// Returns executable names from one local process snapshot without launching
/// any child process or invoking a shell.
#[cfg(windows)]
pub(crate) fn executable_names() -> Result<Vec<String>, ProcessSnapshotError> {
    Ok(executable_images()?
        .into_iter()
        .map(|image| image.executable_name)
        .collect())
}

#[cfg(windows)]
pub(crate) fn executable_images() -> Result<Vec<ProcessImage>, ProcessSnapshotError> {
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };

    struct SnapshotHandle(HANDLE);

    impl Drop for SnapshotHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    let raw_handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw_handle == INVALID_HANDLE_VALUE {
        return Err(ProcessSnapshotError::Unavailable);
    }
    let snapshot = SnapshotHandle(raw_handle);
    let mut entry = unsafe { std::mem::zeroed::<PROCESSENTRY32W>() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut images = Vec::new();

    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
            Ok(images)
        } else {
            Err(ProcessSnapshotError::Failed)
        };
    }

    loop {
        let name_end = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        if name_end != 0 {
            let executable_name = String::from_utf16_lossy(&entry.szExeFile[..name_end]);
            let raw_process =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID) };
            let (executable_path, user_matches_current) = if raw_process.is_null() {
                (None, None)
            } else {
                let process = SnapshotHandle(raw_process);
                let mut buffer = vec![0u16; 32_768];
                let mut length = buffer.len() as u32;
                let executable_path = if unsafe {
                    QueryFullProcessImageNameW(
                        process.0,
                        PROCESS_NAME_WIN32,
                        buffer.as_mut_ptr(),
                        &mut length,
                    )
                } == 0
                {
                    None
                } else {
                    Some(PathBuf::from(String::from_utf16_lossy(
                        &buffer[..length as usize],
                    )))
                };
                let user_matches_current = process_handle_user_matches_current(process.0).ok();
                (executable_path, user_matches_current)
            };
            images.push(ProcessImage {
                process_id: entry.th32ProcessID,
                executable_name,
                executable_path,
                user_matches_current,
            });
        }

        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                Ok(images)
            } else {
                Err(ProcessSnapshotError::Failed)
            };
        }
    }
}

/// Compares the user SID of one already-observed process with the current
/// Agent process. This is used only for the exact configured runner images;
/// it does not enumerate identities or disclose SID text.
#[cfg(all(windows, test))]
pub(crate) fn process_user_matches_current(process_id: u32) -> Result<bool, ProcessSnapshotError> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return Err(ProcessSnapshotError::Unavailable);
    }
    let process = OwnedHandle(process);
    process_handle_user_matches_current(process.0)
}

#[cfg(windows)]
fn process_handle_user_matches_current(
    process: windows_sys::Win32::Foundation::HANDLE,
) -> Result<bool, ProcessSnapshotError> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn token_user_buffer(token: HANDLE) -> Result<Vec<usize>, ProcessSnapshotError> {
        let mut required = 0_u32;
        unsafe {
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut required);
        }
        if required == 0 {
            return Err(ProcessSnapshotError::Failed);
        }
        let unit = std::mem::size_of::<usize>();
        let mut buffer = vec![0_usize; (required as usize).div_ceil(unit)];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err(ProcessSnapshotError::Failed);
        }
        Ok(buffer)
    }

    let mut process_token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut process_token) } == 0 {
        return Err(ProcessSnapshotError::Unavailable);
    }
    let process_token = OwnedHandle(process_token);
    let mut current_token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut current_token) } == 0 {
        return Err(ProcessSnapshotError::Failed);
    }
    let current_token = OwnedHandle(current_token);
    let process_user = token_user_buffer(process_token.0)?;
    let current_user = token_user_buffer(current_token.0)?;
    let process_sid = unsafe { (*(process_user.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    let current_sid = unsafe { (*(current_user.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    Ok(unsafe { EqualSid(process_sid, current_sid) } != 0)
}

/// Resolves the installed binding's opaque execution-identity reference
/// against the current Windows user without persisting or returning SID text.
#[cfg(windows)]
pub(crate) fn current_user_matches_identity_reference(
    reference: &crate::OpaqueIdentityReference,
) -> Result<bool, ProcessSnapshotError> {
    if reference.provider != crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER {
        return Ok(false);
    }
    Ok(current_user_identity_sha256()? == reference.key)
}

#[cfg(windows)]
pub(crate) fn current_user_identity_sha256() -> Result<String, ProcessSnapshotError> {
    use sha2::{Digest, Sha256};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetLengthSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(ProcessSnapshotError::Failed);
    }
    let token = OwnedHandle(token);
    let mut required = 0_u32;
    unsafe {
        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut required);
    }
    if required == 0 {
        return Err(ProcessSnapshotError::Failed);
    }
    let unit = std::mem::size_of::<usize>();
    let mut buffer = vec![0_usize; (required as usize).div_ceil(unit)];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(ProcessSnapshotError::Failed);
    }
    let sid = unsafe { (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    let sid_length = unsafe { GetLengthSid(sid) } as usize;
    if sid_length == 0 {
        return Err(ProcessSnapshotError::Failed);
    }
    let sid_bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), sid_length) };
    Ok(format!("{:x}", Sha256::digest(sid_bytes)))
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::{
        current_user_identity_sha256, current_user_matches_identity_reference, executable_names,
        process_user_matches_current,
    };

    #[test]
    fn native_snapshot_contains_the_current_test_process() {
        let current_exe = std::env::current_exe().expect("test executable path must be available");
        let current_name = current_exe
            .file_name()
            .and_then(|name| name.to_str())
            .expect("test executable name must be valid Unicode");
        let names = executable_names().expect("native process snapshot must succeed");

        assert!(
            names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(current_name)),
            "native snapshot must include the current test process"
        );
    }

    #[test]
    fn current_process_identity_matches_without_serializing_a_sid() {
        assert!(process_user_matches_current(std::process::id()).unwrap());
        let reference = crate::OpaqueIdentityReference::new(
            crate::WINDOWS_SID_SHA256_IDENTITY_PROVIDER,
            current_user_identity_sha256().unwrap(),
        )
        .unwrap();
        assert!(current_user_matches_identity_reference(&reference).unwrap());
    }
}

#[cfg(test)]
mod source_guard_tests {
    #[test]
    fn normal_process_observation_sources_do_not_spawn_commands_or_legacy_utility() {
        let forbidden_command = ["std::process::", "Command"].concat();
        let forbidden_utility = ["task", "list"].concat();
        for source in [
            include_str!("process_snapshot.rs"),
            include_str!("probe.rs"),
            include_str!("runner_observer.rs"),
        ] {
            assert!(
                !source.contains(&forbidden_command),
                "normal process observation must not spawn a command"
            );
            assert!(
                !source.to_ascii_lowercase().contains(&forbidden_utility),
                "normal process observation must not invoke the legacy child-process utility"
            );
        }
    }
}
