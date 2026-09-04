use std::{
    fmt,
    io::{self, BufRead, BufReader, Read, Write},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{AgentCommand, AgentResponse, ReasonCode};

/// The first stable wire protocol version for local Agent IPC.
pub const IPC_PROTOCOL_VERSION: u32 = 1;
#[cfg_attr(not(windows), allow(dead_code))]
const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// A user-local request sent by a CLI or Tray frontend.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IpcRequest {
    pub protocol_version: u32,
    pub request_id: u64,
    pub command: AgentCommand,
}

/// A correlated typed response from the sole Agent authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IpcResponse {
    pub protocol_version: u32,
    pub request_id: u64,
    pub body: IpcResponseBody,
}

/// Success/failure envelope for the IPC wire contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum IpcResponseBody {
    Success(Box<AgentResponse>),
    Failure(IpcError),
}

/// Stable, non-localized IPC failure codes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IpcErrorCode {
    ProtocolVersionMismatch,
    UnauthorizedClient,
    InvalidFrame,
    AgentUnavailable,
    Timeout,
}

/// Machine-readable details for an IPC failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub reason_code: ReasonCode,
    pub supported_protocol_version: Option<u32>,
}

/// Transport-level failure before a typed response can be received.
#[derive(Debug)]
pub enum IpcTransportError {
    Io(io::Error),
    Json(serde_json::Error),
    EmptyFrame,
    FrameTooLarge,
    IncompleteFrame,
    ResponseVersionMismatch { received: u32, expected: u32 },
    UnsupportedPlatform,
    InstanceAlreadyRunning,
    UnauthorizedClient,
    Timeout,
}

impl fmt::Display for IpcTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "IPC I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "IPC JSON failed: {error}"),
            Self::EmptyFrame => formatter.write_str("IPC frame was empty"),
            Self::FrameTooLarge => formatter.write_str("IPC frame exceeded the size limit"),
            Self::IncompleteFrame => formatter.write_str("IPC frame ended before a delimiter"),
            Self::ResponseVersionMismatch { received, expected } => write!(
                formatter,
                "IPC response protocol version {received} does not match {expected}"
            ),
            Self::UnsupportedPlatform => {
                formatter.write_str("local IPC is only implemented on Windows")
            }
            Self::InstanceAlreadyRunning => {
                formatter.write_str("a controlling Agent is already running")
            }
            Self::UnauthorizedClient => {
                formatter.write_str("IPC client identity is not authorized")
            }
            Self::Timeout => formatter.write_str("IPC operation timed out"),
        }
    }
}

impl std::error::Error for IpcTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::EmptyFrame
            | Self::FrameTooLarge
            | Self::IncompleteFrame
            | Self::ResponseVersionMismatch { .. }
            | Self::UnsupportedPlatform
            | Self::InstanceAlreadyRunning
            | Self::UnauthorizedClient
            | Self::Timeout => None,
        }
    }
}

impl From<io::Error> for IpcTransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for IpcTransportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// A local Named Pipe endpoint scoped to an operating-system user identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpcEndpoint {
    scope: String,
}

impl IpcEndpoint {
    /// Resolves an endpoint from the current Windows user SID.
    pub fn for_current_user() -> Result<Self, IpcTransportError> {
        #[cfg(windows)]
        {
            Ok(Self {
                scope: current_user_sid()?,
            })
        }

        #[cfg(not(windows))]
        Err(IpcTransportError::UnsupportedPlatform)
    }

    /// The local Named Pipe path. The scope is intentionally an opaque SID,
    /// not a username or visible presentation string.
    pub fn pipe_name(&self) -> String {
        format!(
            r"\\.\pipe\runnermesh-v{}-{}",
            IPC_PROTOCOL_VERSION, self.scope
        )
    }

    #[cfg(all(test, windows))]
    fn for_test_scope(scope: String) -> Self {
        Self { scope }
    }
}

/// Client for one request/response exchange with the local Agent.
#[derive(Clone, Debug)]
pub struct IpcClient {
    #[cfg_attr(not(windows), allow(dead_code))]
    endpoint: IpcEndpoint,
    #[cfg_attr(not(windows), allow(dead_code))]
    timeout: Duration,
}

impl IpcClient {
    pub fn new(endpoint: IpcEndpoint, timeout: Duration) -> Self {
        Self { endpoint, timeout }
    }

    pub fn call(&self, request: IpcRequest) -> Result<IpcResponse, IpcTransportError> {
        #[cfg(windows)]
        {
            let mut pipe = connect_pipe(&self.endpoint, self.timeout)?;
            write_frame(&mut pipe, &request)?;
            let response: IpcResponse = {
                let mut reader = BufReader::new(&mut pipe);
                read_frame(&mut reader)?
            };
            if response.protocol_version != IPC_PROTOCOL_VERSION {
                return Err(IpcTransportError::ResponseVersionMismatch {
                    received: response.protocol_version,
                    expected: IPC_PROTOCOL_VERSION,
                });
            }
            Ok(response)
        }

        #[cfg(not(windows))]
        {
            let _ = request;
            Err(IpcTransportError::UnsupportedPlatform)
        }
    }
}

/// Listener side of the single Agent authority boundary.
pub struct IpcServer {
    endpoint: IpcEndpoint,
    #[cfg(windows)]
    _guard: InstanceGuard,
}

impl IpcServer {
    /// Acquires the user-scoped controller mutex before accepting clients.
    pub fn bind(endpoint: IpcEndpoint) -> Result<Self, IpcTransportError> {
        #[cfg(windows)]
        {
            let guard = InstanceGuard::acquire(&endpoint)?;
            Ok(Self {
                endpoint,
                _guard: guard,
            })
        }

        #[cfg(not(windows))]
        {
            let _ = endpoint;
            Err(IpcTransportError::UnsupportedPlatform)
        }
    }

    /// Serves one connection. A long-running Agent calls this repeatedly so a
    /// frontend can reconnect after a broken client pipe.
    pub fn serve_once(
        &self,
        handle_command: impl FnMut(AgentCommand) -> Result<AgentResponse, ReasonCode>,
    ) -> Result<(), IpcTransportError> {
        #[cfg(windows)]
        {
            let mut handle_command = handle_command;
            let raw_pipe = create_server_pipe(&self.endpoint)?;
            let connect_result = unsafe {
                windows_sys::Win32::System::Pipes::ConnectNamedPipe(raw_pipe, std::ptr::null_mut())
            };
            if connect_result == 0 {
                let error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                if error != windows_sys::Win32::Foundation::ERROR_PIPE_CONNECTED {
                    unsafe {
                        windows_sys::Win32::Foundation::CloseHandle(raw_pipe);
                    }
                    return Err(IpcTransportError::Io(io::Error::from_raw_os_error(
                        error as i32,
                    )));
                }
            }

            let authorized = client_matches_current_user(raw_pipe)?;
            let mut pipe = unsafe {
                use std::os::windows::io::{FromRawHandle, RawHandle};
                std::fs::File::from_raw_handle(raw_pipe as RawHandle)
            };
            let result = if authorized {
                serve_stream(&mut pipe, &mut handle_command)
            } else {
                write_unauthorized_response(&mut pipe)
            };
            // Dropping the server handle closes this one-shot byte pipe after
            // the response has been flushed. `DisconnectNamedPipe` would
            // discard buffered response bytes before the client can read them.
            result
        }

        #[cfg(not(windows))]
        {
            let _ = handle_command;
            Err(IpcTransportError::UnsupportedPlatform)
        }
    }

    pub fn endpoint(&self) -> &IpcEndpoint {
        &self.endpoint
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn serve_stream<S>(
    stream: &mut S,
    handle_command: &mut impl FnMut(AgentCommand) -> Result<AgentResponse, ReasonCode>,
) -> Result<(), IpcTransportError>
where
    S: Read + Write,
{
    let request: IpcRequest = {
        let mut reader = BufReader::new(&mut *stream);
        read_frame(&mut reader)?
    };
    let response = if request.protocol_version != IPC_PROTOCOL_VERSION {
        IpcResponse {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: request.request_id,
            body: IpcResponseBody::Failure(IpcError {
                code: IpcErrorCode::ProtocolVersionMismatch,
                reason_code: static_reason("protocol-version-mismatch"),
                supported_protocol_version: Some(IPC_PROTOCOL_VERSION),
            }),
        }
    } else {
        let body = match handle_command(request.command) {
            Ok(response) => IpcResponseBody::Success(Box::new(response)),
            Err(reason_code) => IpcResponseBody::Failure(IpcError {
                code: IpcErrorCode::AgentUnavailable,
                reason_code,
                supported_protocol_version: None,
            }),
        };
        IpcResponse {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: request.request_id,
            body,
        }
    };
    write_frame(stream, &response)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn write_unauthorized_response(stream: &mut impl Write) -> Result<(), IpcTransportError> {
    let response = IpcResponse {
        protocol_version: IPC_PROTOCOL_VERSION,
        request_id: 0,
        body: IpcResponseBody::Failure(IpcError {
            code: IpcErrorCode::UnauthorizedClient,
            reason_code: static_reason("client-identity-mismatch"),
            supported_protocol_version: None,
        }),
    };
    write_frame(stream, &response)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn read_frame<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
) -> Result<T, IpcTransportError> {
    let mut bytes = Vec::new();
    let bytes_read = reader
        .take((MAX_FRAME_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes_read == 0 {
        return Err(IpcTransportError::EmptyFrame);
    }
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(IpcTransportError::FrameTooLarge);
    }
    if bytes.last() != Some(&b'\n') {
        return Err(IpcTransportError::IncompleteFrame);
    }
    bytes.pop();
    if bytes.is_empty() {
        return Err(IpcTransportError::EmptyFrame);
    }
    serde_json::from_slice(&bytes).map_err(IpcTransportError::Json)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), IpcTransportError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(IpcTransportError::FrameTooLarge);
    }
    writer.write_all(&bytes)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn static_reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static IPC reason codes must be valid")
}

#[cfg(windows)]
struct InstanceGuard(isize);

#[cfg(windows)]
impl InstanceGuard {
    fn acquire(endpoint: &IpcEndpoint) -> Result<Self, IpcTransportError> {
        use windows_sys::Win32::Foundation::{
            CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE,
        };
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let name = to_wide(&format!(
            r"Local\runnermesh-agent-v{}-{}",
            IPC_PROTOCOL_VERSION, endpoint.scope
        ));
        let handle: HANDLE = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if handle.is_null() {
            return Err(IpcTransportError::Io(io::Error::last_os_error()));
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }
            return Err(IpcTransportError::InstanceAlreadyRunning);
        }
        Ok(Self(handle as isize))
    }
}

#[cfg(windows)]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}

/// Owns the LocalAlloc-backed descriptor for exactly one CreateNamedPipeW
/// call. The descriptor contains a protected DACL with one allow ACE for the
/// current user SID; the post-connect token comparison remains defense in
/// depth.
#[cfg(windows)]
struct CurrentUserPipeSecurity(windows_sys::Win32::Security::PSECURITY_DESCRIPTOR);

#[cfg(windows)]
impl CurrentUserPipeSecurity {
    fn new() -> Result<Self, IpcTransportError> {
        use windows_sys::Win32::Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR,
        };

        let sddl = pipe_security_sddl(&current_user_sid()?)?;
        let sddl = to_wide(&sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(IpcTransportError::Io(io::Error::last_os_error()));
        }
        Ok(Self(descriptor))
    }

    fn attributes(&mut self) -> windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
        windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<windows_sys::Win32::Security::SECURITY_ATTRIBUTES>()
                as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

#[cfg(windows)]
impl Drop for CurrentUserPipeSecurity {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::LocalFree(self.0);
            }
        }
    }
}

#[cfg(windows)]
fn pipe_security_sddl(sid: &str) -> Result<String, IpcTransportError> {
    let mut parts = sid.split('-');
    if sid.len() > 256
        || parts.next() != Some("S")
        || parts.clone().count() < 2
        || parts.any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(IpcTransportError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "current user SID is not canonical",
        )));
    }
    Ok(format!("D:P(A;;GA;;;{sid})"))
}

#[cfg(windows)]
fn create_server_pipe(
    endpoint: &IpcEndpoint,
) -> Result<windows_sys::Win32::Foundation::HANDLE, IpcTransportError> {
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Storage::FileSystem::PIPE_ACCESS_DUPLEX,
        System::Pipes::{
            CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
            PIPE_WAIT,
        },
    };

    let name = to_wide(&endpoint.pipe_name());
    let mut security = CurrentUserPipeSecurity::new()?;
    let attributes = security.attributes();
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            MAX_FRAME_BYTES as u32,
            MAX_FRAME_BYTES as u32,
            0,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(IpcTransportError::Io(io::Error::last_os_error()))
    } else {
        Ok(handle)
    }
}

#[cfg(windows)]
fn connect_pipe(
    endpoint: &IpcEndpoint,
    timeout: Duration,
) -> Result<std::fs::File, IpcTransportError> {
    use std::{
        os::windows::io::{FromRawHandle, RawHandle},
        thread,
        time::Instant,
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            OPEN_EXISTING,
        },
        System::Pipes::WaitNamedPipeW,
    };

    let started = Instant::now();
    let name = to_wide(&endpoint.pipe_name());
    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(IpcTransportError::Timeout);
        }
        let wait_millis = remaining.as_millis().min(25) as u32;
        let _ = unsafe { WaitNamedPipeW(name.as_ptr(), wait_millis) };
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            return Ok(unsafe { std::fs::File::from_raw_handle(handle as RawHandle) });
        }
        let error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        if error != ERROR_PIPE_BUSY && error != ERROR_FILE_NOT_FOUND {
            return Err(IpcTransportError::Io(io::Error::from_raw_os_error(
                error as i32,
            )));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(windows)]
fn client_matches_current_user(
    pipe: windows_sys::Win32::Foundation::HANDLE,
) -> Result<bool, IpcTransportError> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{EqualSid, TOKEN_QUERY},
        System::{
            Pipes::GetNamedPipeClientProcessId,
            Threading::{
                GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };

    let mut client_pid = 0;
    if unsafe { GetNamedPipeClientProcessId(pipe, &mut client_pid) } == 0 {
        return Err(IpcTransportError::Io(io::Error::last_os_error()));
    }
    let client_process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, client_pid) };
    if client_process.is_null() {
        return Err(IpcTransportError::Io(io::Error::last_os_error()));
    }

    let result = (|| {
        let mut client_token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(client_process, TOKEN_QUERY, &mut client_token) } == 0 {
            return Err(IpcTransportError::Io(io::Error::last_os_error()));
        }
        let mut current_token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut current_token) } == 0 {
            unsafe {
                CloseHandle(client_token);
            }
            return Err(IpcTransportError::Io(io::Error::last_os_error()));
        }
        let comparison = with_token_user_sid(client_token, |client_sid| {
            with_token_user_sid(current_token, |current_sid| {
                Ok(unsafe { EqualSid(client_sid, current_sid) } != 0)
            })
        });
        unsafe {
            CloseHandle(current_token);
            CloseHandle(client_token);
        }
        comparison
    })();
    unsafe {
        CloseHandle(client_process);
    }
    result
}

#[cfg(windows)]
fn current_user_sid() -> Result<String, IpcTransportError> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::Authorization::ConvertSidToStringSidW,
        Security::TOKEN_QUERY,
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(IpcTransportError::Io(io::Error::last_os_error()));
    }
    let result = with_token_user_sid(token, |sid| {
        let mut sid_text = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut sid_text) } == 0 {
            return Err(IpcTransportError::Io(io::Error::last_os_error()));
        }
        let value = unsafe { wide_ptr_to_string(sid_text) };
        unsafe {
            LocalFree(sid_text.cast());
        }
        Ok(value)
    });
    unsafe {
        CloseHandle(token);
    }
    result
}

#[cfg(windows)]
fn with_token_user_sid<T>(
    token: windows_sys::Win32::Foundation::HANDLE,
    action: impl FnOnce(windows_sys::Win32::Security::PSID) -> Result<T, IpcTransportError>,
) -> Result<T, IpcTransportError> {
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_USER};

    let mut required_bytes = 0;
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut required_bytes,
        );
    }
    if required_bytes == 0 {
        return Err(IpcTransportError::Io(io::Error::last_os_error()));
    }
    let mut buffer = vec![0_u8; required_bytes as usize];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required_bytes,
            &mut required_bytes,
        )
    } == 0
    {
        return Err(IpcTransportError::Io(io::Error::last_os_error()));
    }
    let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    action(token_user.User.Sid)
}

#[cfg(windows)]
fn to_wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
unsafe fn wide_ptr_to_string(pointer: *const u16) -> String {
    let mut length = 0;
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, length) })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    #[cfg(windows)]
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(windows)]
    use super::IpcEndpoint;
    use super::{
        read_frame, serve_stream, write_frame, IpcErrorCode, IpcRequest, IpcResponse,
        IpcResponseBody, IpcTransportError, IPC_PROTOCOL_VERSION,
    };
    #[cfg(windows)]
    use crate::AgentResponse;
    use crate::{AgentCommand, ReasonCode};

    #[test]
    fn framing_round_trips_typed_contracts() {
        let request = IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: 41,
            command: AgentCommand::GetSnapshot,
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &request).unwrap();
        let decoded: IpcRequest = read_frame(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn incomplete_and_oversized_frames_are_rejected() {
        let error = read_frame::<IpcRequest>(&mut Cursor::new(b"{}".to_vec())).unwrap_err();
        assert!(matches!(error, IpcTransportError::IncompleteFrame));

        let oversized = vec![b'x'; super::MAX_FRAME_BYTES + 1];
        let error = read_frame::<IpcRequest>(&mut Cursor::new(oversized)).unwrap_err();
        assert!(matches!(error, IpcTransportError::FrameTooLarge));
    }

    #[test]
    fn protocol_mismatch_receives_a_typed_failure() {
        let request = IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION + 1,
            request_id: 7,
            command: AgentCommand::GetSnapshot,
        };
        let mut stream = Cursor::new(frame_bytes(&request));
        serve_stream(&mut stream, &mut |_| {
            unreachable!("version must be rejected first")
        })
        .unwrap();
        stream.set_position(request_frame_len(&request) as u64);
        let response: IpcResponse = read_frame(&mut stream).unwrap();

        assert_eq!(response.request_id, 7);
        assert!(matches!(
            response.body,
            IpcResponseBody::Failure(ref failure)
                if failure.code == IpcErrorCode::ProtocolVersionMismatch
                    && failure.supported_protocol_version == Some(IPC_PROTOCOL_VERSION)
        ));
    }

    #[test]
    fn agent_failure_is_a_typed_response_not_a_transport_string() {
        let request = IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: 9,
            command: AgentCommand::GetSnapshot,
        };
        let mut stream = Cursor::new(frame_bytes(&request));
        serve_stream(&mut stream, &mut |_| {
            Err(ReasonCode::new("agent-starting").unwrap())
        })
        .unwrap();
        stream.set_position(request_frame_len(&request) as u64);
        let response: IpcResponse = read_frame(&mut stream).unwrap();
        assert!(matches!(
            response.body,
            IpcResponseBody::Failure(ref failure)
                if failure.code == IpcErrorCode::AgentUnavailable
                    && failure.reason_code.as_str() == "agent-starting"
        ));
    }

    fn frame_bytes(request: &IpcRequest) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, request).unwrap();
        bytes
    }

    fn request_frame_len(request: &IpcRequest) -> usize {
        frame_bytes(request).len()
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_round_trips_reconnects_and_rejects_a_second_controller() {
        use std::{thread, time::Duration};

        use super::{IpcClient, IpcServer};

        let endpoint = test_endpoint();
        let server = IpcServer::bind(endpoint.clone()).unwrap();
        assert!(matches!(
            IpcServer::bind(endpoint.clone()),
            Err(IpcTransportError::InstanceAlreadyRunning)
        ));

        for request_id in [1, 2] {
            thread::scope(|scope| {
                let server_task = scope.spawn(|| {
                    server.serve_once(|command| {
                        assert_eq!(command, AgentCommand::GetVersion);
                        Ok(AgentResponse::Rejected {
                            reason_code: ReasonCode::new("not-implemented").unwrap(),
                        })
                    })
                });
                let client_result =
                    IpcClient::new(endpoint.clone(), Duration::from_secs(2)).call(IpcRequest {
                        protocol_version: IPC_PROTOCOL_VERSION,
                        request_id,
                        command: AgentCommand::GetVersion,
                    });
                let server_result = server_task.join().unwrap();
                assert!(server_result.is_ok(), "server result: {server_result:?}");
                let response = client_result.unwrap();
                assert_eq!(response.request_id, request_id);
                assert!(matches!(response.body, IpcResponseBody::Success(_)));
            });
        }
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_dacl_is_current_user_only_and_denies_anonymous_open() {
        use std::time::Duration;

        use windows_sys::Win32::{
            Foundation::{CloseHandle, ERROR_ACCESS_DENIED, HANDLE},
            Security::{ImpersonateAnonymousToken, RevertToSelf},
            System::Threading::GetCurrentThread,
        };

        struct PipeHandle(HANDLE);
        impl Drop for PipeHandle {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }

        struct AnonymousImpersonation(bool);
        impl AnonymousImpersonation {
            fn begin() -> std::io::Result<Self> {
                if unsafe { ImpersonateAnonymousToken(GetCurrentThread()) } == 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(Self(true))
                }
            }

            fn revert(mut self) -> std::io::Result<()> {
                let result = if unsafe { RevertToSelf() } == 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                };
                self.0 = false;
                result
            }
        }
        impl Drop for AnonymousImpersonation {
            fn drop(&mut self) {
                if self.0 {
                    unsafe {
                        RevertToSelf();
                    }
                }
            }
        }

        let synthetic = super::pipe_security_sddl("S-1-5-21-1-2-3-1001").unwrap();
        assert_eq!(synthetic, "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)");
        assert!(!synthetic.contains(";;;WD"));
        assert!(!synthetic.contains(";;;AN"));
        assert!(!synthetic.contains(";;;AU"));
        assert!(!synthetic.contains(";;;BU"));

        let endpoint = test_endpoint();
        let _pipe = PipeHandle(super::create_server_pipe(&endpoint).unwrap());
        let impersonation = AnonymousImpersonation::begin().unwrap();
        let result = super::connect_pipe(&endpoint, Duration::from_millis(250));
        impersonation.revert().unwrap();
        assert!(matches!(
            result,
            Err(IpcTransportError::Io(ref error))
                if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32)
        ));
    }

    #[cfg(windows)]
    fn test_endpoint() -> IpcEndpoint {
        static NEXT_SCOPE: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT_SCOPE.fetch_add(1, Ordering::Relaxed);
        let current = IpcEndpoint::for_current_user().unwrap();
        IpcEndpoint::for_test_scope(format!("{}-test-{unique}", current.scope))
    }
}
