//! Read-only executable-name and exact-image enumeration for local process
//! observation. Neither path ever launches a child process or invokes a shell.

use std::path::PathBuf;

/// Deliberately small failure surface for consumers that must not expose OS
/// error details through stable RunnerMesh contracts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessSnapshotError {
    Unavailable,
    #[cfg(windows)]
    Failed,
}

/// One image observed in a point-in-time local process snapshot. Missing path
/// evidence never grants exact-runner authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessImage {
    pub process_id: u32,
    pub executable_name: String,
    pub executable_path: Option<PathBuf>,
}

/// Returns executable names from one local process snapshot without launching
/// any child process or invoking a shell.
pub(crate) fn executable_names() -> Result<Vec<String>, ProcessSnapshotError> {
    #[cfg(windows)]
    {
        Ok(executable_images()?
            .into_iter()
            .map(|image| image.executable_name)
            .collect())
    }

    #[cfg(not(windows))]
    {
        Err(ProcessSnapshotError::Unavailable)
    }
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
            let executable_path = if raw_process.is_null() {
                None
            } else {
                let process = SnapshotHandle(raw_process);
                let mut buffer = vec![0u16; 32_768];
                let mut length = buffer.len() as u32;
                if unsafe {
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
                }
            };
            images.push(ProcessImage {
                process_id: entry.th32ProcessID,
                executable_name,
                executable_path,
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

#[cfg(all(test, windows))]
mod windows_tests {
    use super::executable_names;

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
