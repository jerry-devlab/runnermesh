//! Read-only executable-name enumeration for local process observation.

/// Deliberately small failure surface for consumers that must not expose OS
/// error details through stable RunnerMesh contracts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessSnapshotError {
    Unavailable,
    #[cfg(windows)]
    Failed,
}

/// Returns executable names from one local process snapshot without launching
/// any child process or invoking a shell.
pub(crate) fn executable_names() -> Result<Vec<String>, ProcessSnapshotError> {
    #[cfg(windows)]
    {
        windows_executable_names()
    }

    #[cfg(not(windows))]
    {
        Err(ProcessSnapshotError::Unavailable)
    }
}

#[cfg(windows)]
fn windows_executable_names() -> Result<Vec<String>, ProcessSnapshotError> {
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
        },
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
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
    let mut names = Vec::new();

    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
            Ok(names)
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
            names.push(String::from_utf16_lossy(&entry.szExeFile[..name_end]));
        }

        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                Ok(names)
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
    fn normal_process_observation_sources_do_not_spawn_commands() {
        for source in [include_str!("probe.rs"), include_str!("runner_observer.rs")] {
            assert!(
                !source.contains("std::process::Command"),
                "normal process observation must not spawn a command"
            );
        }
    }
}
