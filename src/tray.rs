use std::{fmt, thread::ThreadId};

use crate::{
    AgentCommand, AgentResponse, AgentSnapshot, AgentTransport, DoctorReport, IpcRequest,
    IpcResponseBody, IpcTransportError, LanguagePreference, LinkKind, LinkState, ProbeId,
    ThemePreference, UiPreferences, UserMode, ZenOverride, IPC_PROTOCOL_VERSION,
};

/// Stable identifiers for native tray actions. They are never localized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrayMenuId {
    Version,
    AgentHealth,
    Capacity,
    RunnerPhase,
    GithubLink,
    Zen,
    Mode(UserMode),
    ProbeToggle(ProbeId),
    StatusDetails,
    Doctor,
    Logs,
    Theme(ThemePreference),
    Language(LanguagePreference),
    StartOnLogin,
    IdleThreshold(u64),
    UpdateChecks,
    OpenConfig,
    OpenDataDirectory,
    CheckForUpdates,
    ExitAfterDrain,
}

impl TrayMenuId {
    pub fn stable_id(&self) -> String {
        match self {
            Self::Version => "status.version".to_owned(),
            Self::AgentHealth => "status.agent-health".to_owned(),
            Self::Capacity => "status.capacity".to_owned(),
            Self::RunnerPhase => "status.runner-phase".to_owned(),
            Self::GithubLink => "status.github-link".to_owned(),
            Self::Zen => "control.zen".to_owned(),
            Self::Mode(mode) => format!("control.mode.{mode}"),
            Self::ProbeToggle(probe_id) => format!("control.probe.{probe_id}"),
            Self::StatusDetails => "action.status-details".to_owned(),
            Self::Doctor => "action.doctor".to_owned(),
            Self::Logs => "action.logs".to_owned(),
            Self::Theme(ThemePreference::System) => "settings.theme.system".to_owned(),
            Self::Theme(ThemePreference::Light) => "settings.theme.light".to_owned(),
            Self::Theme(ThemePreference::Dark) => "settings.theme.dark".to_owned(),
            Self::Language(LanguagePreference::System) => "settings.language.system".to_owned(),
            Self::Language(LanguagePreference::ZhCn) => "settings.language.zh-CN".to_owned(),
            Self::Language(LanguagePreference::EnUs) => "settings.language.en-US".to_owned(),
            Self::StartOnLogin => "settings.start-on-login".to_owned(),
            Self::IdleThreshold(seconds) => format!("settings.idle-threshold.{seconds}"),
            Self::UpdateChecks => "settings.update-checks".to_owned(),
            Self::OpenConfig => "settings.open-config".to_owned(),
            Self::OpenDataDirectory => "settings.open-data-directory".to_owned(),
            Self::CheckForUpdates => "action.check-for-updates".to_owned(),
            Self::ExitAfterDrain => "action.exit-after-drain".to_owned(),
        }
    }
}

/// A complete presentation model for a native menu backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrayMenuEntry {
    Item(TrayMenuItem),
    Separator,
    Submenu {
        id: String,
        label: String,
        entries: Vec<TrayMenuEntry>,
    },
}

/// Visible state is separate from the stable menu identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrayMenuItem {
    pub id: TrayMenuId,
    pub label: String,
    pub checked: Option<bool>,
    pub enabled: bool,
}

/// Capacity glyph semantics. Color may supplement these glyphs but never carry
/// the state by itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayIconGlyph {
    Full,
    Throttled,
    Drained,
    Offline,
}

impl TrayIconGlyph {
    pub fn marker(self) -> char {
        match self {
            Self::Full => 'F',
            Self::Throttled => 'T',
            Self::Drained => 'D',
            Self::Offline => 'O',
        }
    }
}

/// Rendered native-tray state. It has no policy authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrayRender {
    pub icon_glyph: TrayIconGlyph,
    pub tooltip: String,
    pub entries: Vec<TrayMenuEntry>,
}

/// Typed message sent from background Agent/IPC work to the UI event-loop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrayUiUpdate {
    Snapshot(AgentSnapshot),
}

/// Result of a stable menu action after the Agent responds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrayActionResult {
    Snapshot(AgentSnapshot),
    Doctor(DoctorReport),
    Rejected(crate::ReasonCode),
    NoOp,
}

#[derive(Debug)]
pub enum TrayError {
    WrongUiThread,
    Transport(IpcTransportError),
    ResponseCorrelation { expected: u64, received: u64 },
}

impl fmt::Display for TrayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongUiThread => {
                formatter.write_str("tray mutation must occur on its UI event-loop thread")
            }
            Self::Transport(error) => write!(formatter, "Agent IPC unavailable: {error}"),
            Self::ResponseCorrelation { expected, received } => {
                write!(
                    formatter,
                    "Agent response {received} did not match tray request {expected}"
                )
            }
        }
    }
}

impl std::error::Error for TrayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::WrongUiThread | Self::ResponseCorrelation { .. } => None,
        }
    }
}

/// Native-lightweight event-loop adapter. A Windows native tray backend owns
/// OS widget mutation and calls these methods from its event-loop thread only.
/// Background Agent work communicates through [`TrayUiUpdate`] instead.
pub struct NativeTrayEventLoop<T> {
    transport: T,
    ui_thread: ThreadId,
    current_render: Option<TrayRender>,
}

impl<T: AgentTransport> NativeTrayEventLoop<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            ui_thread: std::thread::current().id(),
            current_render: None,
        }
    }

    /// Replaces the full native menu/icon model from the one owning thread.
    pub fn apply(&mut self, update: TrayUiUpdate) -> Result<&TrayRender, TrayError> {
        self.require_ui_thread()?;
        let TrayUiUpdate::Snapshot(snapshot) = update;
        self.current_render = Some(render(&snapshot));
        Ok(self
            .current_render
            .as_ref()
            .expect("render was just assigned"))
    }

    pub fn current_render(&self) -> Option<&TrayRender> {
        self.current_render.as_ref()
    }

    /// Sends one typed Agent command for a stable menu identifier. The tray
    /// never decides runner, admission, or probe state locally.
    pub fn activate(
        &mut self,
        id: &TrayMenuId,
        snapshot: &AgentSnapshot,
    ) -> Result<TrayActionResult, TrayError> {
        self.require_ui_thread()?;
        let Some(command) = command_for(id, snapshot) else {
            return Ok(TrayActionResult::NoOp);
        };
        let response = send(&self.transport, command)?;
        match response {
            AgentResponse::Snapshot(snapshot) | AgentResponse::Accepted { snapshot } => {
                self.current_render = Some(render(&snapshot));
                Ok(TrayActionResult::Snapshot(snapshot))
            }
            AgentResponse::Doctor(report) => Ok(TrayActionResult::Doctor(report)),
            AgentResponse::Rejected { reason_code } => Ok(TrayActionResult::Rejected(reason_code)),
        }
    }

    fn require_ui_thread(&self) -> Result<(), TrayError> {
        if std::thread::current().id() == self.ui_thread {
            Ok(())
        } else {
            Err(TrayError::WrongUiThread)
        }
    }
}

fn command_for(id: &TrayMenuId, snapshot: &AgentSnapshot) -> Option<AgentCommand> {
    match id {
        TrayMenuId::Zen => Some(AgentCommand::SetZen {
            zen: match snapshot.zen {
                ZenOverride::Disabled => ZenOverride::Enabled,
                ZenOverride::Enabled => ZenOverride::Disabled,
            },
        }),
        TrayMenuId::Mode(mode) => Some(AgentCommand::SetMode { mode: *mode }),
        TrayMenuId::ProbeToggle(probe_id) => snapshot
            .probes
            .iter()
            .find(|probe| probe.id == *probe_id)
            .map(|probe| AgentCommand::SetProbeEnabled {
                probe_id: probe_id.clone(),
                enabled: !probe.enabled,
            }),
        TrayMenuId::StatusDetails => Some(AgentCommand::GetSnapshot),
        TrayMenuId::Doctor => Some(AgentCommand::RunDoctor),
        TrayMenuId::Logs => Some(AgentCommand::OpenLogs),
        TrayMenuId::Theme(theme) => Some(AgentCommand::SetUiPreferences {
            ui_preferences: UiPreferences {
                theme: *theme,
                language: snapshot.ui_preferences.language,
            },
        }),
        TrayMenuId::Language(language) => Some(AgentCommand::SetUiPreferences {
            ui_preferences: UiPreferences {
                theme: snapshot.ui_preferences.theme,
                language: *language,
            },
        }),
        TrayMenuId::StartOnLogin => Some(AgentCommand::SetStartOnLoginPreference {
            enabled: !snapshot.start_on_login_preference,
        }),
        TrayMenuId::IdleThreshold(seconds) => {
            Some(AgentCommand::SetAutoIdleThreshold { seconds: *seconds })
        }
        TrayMenuId::UpdateChecks => Some(AgentCommand::SetUpdateChecksEnabled {
            enabled: !snapshot.update_checks_enabled,
        }),
        TrayMenuId::OpenConfig => Some(AgentCommand::OpenConfig),
        TrayMenuId::OpenDataDirectory => Some(AgentCommand::OpenDataDirectory),
        TrayMenuId::CheckForUpdates => Some(AgentCommand::CheckForUpdates),
        TrayMenuId::ExitAfterDrain => Some(AgentCommand::ExitAfterDrain),
        TrayMenuId::Version
        | TrayMenuId::AgentHealth
        | TrayMenuId::Capacity
        | TrayMenuId::RunnerPhase
        | TrayMenuId::GithubLink => None,
    }
}

fn send(
    transport: &impl AgentTransport,
    command: AgentCommand,
) -> Result<AgentResponse, TrayError> {
    const REQUEST_ID: u64 = 1;
    let response = transport
        .call(IpcRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: REQUEST_ID,
            command,
        })
        .map_err(TrayError::Transport)?;
    if response.request_id != REQUEST_ID {
        return Err(TrayError::ResponseCorrelation {
            expected: REQUEST_ID,
            received: response.request_id,
        });
    }
    match response.body {
        IpcResponseBody::Success(response) => Ok(response),
        IpcResponseBody::Failure(error) => Ok(AgentResponse::Rejected {
            reason_code: error.reason_code,
        }),
    }
}

fn render(snapshot: &AgentSnapshot) -> TrayRender {
    let language = snapshot.ui_preferences.language;
    let github = snapshot
        .links
        .iter()
        .find(|link| link.kind == LinkKind::GithubActions)
        .map(|link| link.state)
        .unwrap_or(LinkState::Unknown);
    let icon_glyph = match snapshot.node_state {
        crate::NodeState::Full => TrayIconGlyph::Full,
        crate::NodeState::Throttled => TrayIconGlyph::Throttled,
        crate::NodeState::Drained => TrayIconGlyph::Drained,
        crate::NodeState::Offline => TrayIconGlyph::Offline,
    };
    TrayRender {
        icon_glyph,
        tooltip: format!(
            "RunnerMesh {} · {} · {}",
            icon_glyph.marker(),
            snapshot.node_state,
            snapshot.user_mode
        ),
        entries: vec![
            item(
                TrayMenuId::Version,
                format!(
                    "RunnerMesh {} · {}",
                    snapshot.build.version, snapshot.build.channel
                ),
                None,
                false,
            ),
            item(
                TrayMenuId::AgentHealth,
                format!("{}: {}", text(language, Text::Agent), snapshot.health),
                None,
                false,
            ),
            item(
                TrayMenuId::Capacity,
                format!(
                    "{}: {} · {}",
                    text(language, Text::Capacity),
                    snapshot.node_state,
                    snapshot.user_mode
                ),
                None,
                false,
            ),
            item(
                TrayMenuId::RunnerPhase,
                format!(
                    "{}: {}",
                    text(language, Text::Runner),
                    snapshot.runner_phase
                ),
                None,
                false,
            ),
            item(
                TrayMenuId::GithubLink,
                format!("{}: {}", text(language, Text::Github), github),
                None,
                false,
            ),
            TrayMenuEntry::Separator,
            item(
                TrayMenuId::Zen,
                text(language, Text::Zen).to_owned(),
                Some(snapshot.zen == ZenOverride::Enabled),
                true,
            ),
            submenu(
                "control.mode",
                text(language, Text::Mode),
                mode_entries(snapshot, language),
            ),
            submenu(
                "control.probes",
                text(language, Text::Probes),
                probe_entries(snapshot, language),
            ),
            TrayMenuEntry::Separator,
            item(
                TrayMenuId::StatusDetails,
                text(language, Text::StatusDetails).to_owned(),
                None,
                true,
            ),
            item(
                TrayMenuId::Doctor,
                text(language, Text::Doctor).to_owned(),
                None,
                true,
            ),
            item(
                TrayMenuId::Logs,
                text(language, Text::Logs).to_owned(),
                None,
                true,
            ),
            submenu(
                "settings",
                text(language, Text::Settings),
                settings_entries(snapshot, language),
            ),
            TrayMenuEntry::Separator,
            item(
                TrayMenuId::CheckForUpdates,
                text(language, Text::CheckForUpdates).to_owned(),
                None,
                true,
            ),
            item(
                TrayMenuId::ExitAfterDrain,
                text(language, Text::ExitAfterDrain).to_owned(),
                None,
                true,
            ),
        ],
    }
}

fn mode_entries(snapshot: &AgentSnapshot, language: LanguagePreference) -> Vec<TrayMenuEntry> {
    [
        UserMode::Auto,
        UserMode::Work,
        UserMode::Gaming,
        UserMode::Idle,
        UserMode::Maintenance,
        UserMode::ForceCi,
    ]
    .into_iter()
    .map(|mode| {
        item(
            TrayMenuId::Mode(mode),
            mode_label(language, mode).to_owned(),
            Some(snapshot.user_mode == mode),
            true,
        )
    })
    .collect()
}

fn probe_entries(snapshot: &AgentSnapshot, language: LanguagePreference) -> Vec<TrayMenuEntry> {
    snapshot
        .probes
        .iter()
        .map(|probe| {
            item(
                TrayMenuId::ProbeToggle(probe.id.clone()),
                format!(
                    "{} · {}",
                    probe_label(language, &probe.id),
                    probe.runtime_state
                ),
                Some(probe.enabled),
                true,
            )
        })
        .collect()
}

fn settings_entries(snapshot: &AgentSnapshot, language: LanguagePreference) -> Vec<TrayMenuEntry> {
    vec![
        submenu(
            "settings.theme",
            text(language, Text::Appearance),
            [
                ThemePreference::System,
                ThemePreference::Light,
                ThemePreference::Dark,
            ]
            .into_iter()
            .map(|theme| {
                item(
                    TrayMenuId::Theme(theme),
                    theme_label(language, theme).to_owned(),
                    Some(snapshot.ui_preferences.theme == theme),
                    true,
                )
            })
            .collect(),
        ),
        submenu(
            "settings.language",
            text(language, Text::Language),
            [
                LanguagePreference::System,
                LanguagePreference::ZhCn,
                LanguagePreference::EnUs,
            ]
            .into_iter()
            .map(|choice| {
                item(
                    TrayMenuId::Language(choice),
                    language_label(language, choice).to_owned(),
                    Some(snapshot.ui_preferences.language == choice),
                    true,
                )
            })
            .collect(),
        ),
        item(
            TrayMenuId::StartOnLogin,
            text(language, Text::StartOnLogin).to_owned(),
            Some(snapshot.start_on_login_preference),
            true,
        ),
        submenu(
            "settings.idle-threshold",
            text(language, Text::IdleThreshold),
            [300_u64, 600, 900]
                .into_iter()
                .map(|seconds| {
                    item(
                        TrayMenuId::IdleThreshold(seconds),
                        format!("{} min", seconds / 60),
                        Some(snapshot.auto_idle_threshold_seconds == seconds),
                        true,
                    )
                })
                .collect(),
        ),
        item(
            TrayMenuId::UpdateChecks,
            text(language, Text::UpdateChecks).to_owned(),
            Some(snapshot.update_checks_enabled),
            true,
        ),
        item(
            TrayMenuId::OpenConfig,
            text(language, Text::OpenConfig).to_owned(),
            None,
            true,
        ),
        item(
            TrayMenuId::OpenDataDirectory,
            text(language, Text::OpenDataDirectory).to_owned(),
            None,
            true,
        ),
    ]
}

fn item(id: TrayMenuId, label: String, checked: Option<bool>, enabled: bool) -> TrayMenuEntry {
    TrayMenuEntry::Item(TrayMenuItem {
        id,
        label,
        checked,
        enabled,
    })
}

fn submenu(id: &str, label: &str, entries: Vec<TrayMenuEntry>) -> TrayMenuEntry {
    TrayMenuEntry::Submenu {
        id: id.to_owned(),
        label: label.to_owned(),
        entries,
    }
}

#[derive(Clone, Copy)]
enum Text {
    Agent,
    Capacity,
    Runner,
    Github,
    Zen,
    Mode,
    Probes,
    StatusDetails,
    Doctor,
    Logs,
    Settings,
    Appearance,
    Language,
    StartOnLogin,
    IdleThreshold,
    UpdateChecks,
    OpenConfig,
    OpenDataDirectory,
    CheckForUpdates,
    ExitAfterDrain,
}

fn text(language: LanguagePreference, key: Text) -> &'static str {
    let chinese = language == LanguagePreference::ZhCn;
    match (chinese, key) {
        (false, Text::Agent) => "Agent",
        (false, Text::Capacity) => "Capacity",
        (false, Text::Runner) => "Runner",
        (false, Text::Github) => "GitHub",
        (false, Text::Zen) => "Zen Mode",
        (false, Text::Mode) => "Mode",
        (false, Text::Probes) => "Probes",
        (false, Text::StatusDetails) => "Status details",
        (false, Text::Doctor) => "Doctor",
        (false, Text::Logs) => "Logs",
        (false, Text::Settings) => "Settings",
        (false, Text::Appearance) => "Appearance",
        (false, Text::Language) => "Language",
        (false, Text::StartOnLogin) => "Start on login",
        (false, Text::IdleThreshold) => "Auto idle threshold",
        (false, Text::UpdateChecks) => "Check for updates automatically",
        (false, Text::OpenConfig) => "Open config",
        (false, Text::OpenDataDirectory) => "Open data directory",
        (false, Text::CheckForUpdates) => "Check for updates",
        (false, Text::ExitAfterDrain) => "Exit after drain",
        (true, Text::Agent) => "代理",
        (true, Text::Capacity) => "容量",
        (true, Text::Runner) => "运行器",
        (true, Text::Github) => "GitHub",
        (true, Text::Zen) => "禅模式",
        (true, Text::Mode) => "模式",
        (true, Text::Probes) => "探针",
        (true, Text::StatusDetails) => "状态详情",
        (true, Text::Doctor) => "诊断",
        (true, Text::Logs) => "日志",
        (true, Text::Settings) => "设置",
        (true, Text::Appearance) => "外观",
        (true, Text::Language) => "语言",
        (true, Text::StartOnLogin) => "登录时启动",
        (true, Text::IdleThreshold) => "自动空闲阈值",
        (true, Text::UpdateChecks) => "自动检查更新",
        (true, Text::OpenConfig) => "打开配置",
        (true, Text::OpenDataDirectory) => "打开数据目录",
        (true, Text::CheckForUpdates) => "检查更新",
        (true, Text::ExitAfterDrain) => "排空后退出",
    }
}

fn mode_label(language: LanguagePreference, mode: UserMode) -> &'static str {
    match (language == LanguagePreference::ZhCn, mode) {
        (false, UserMode::Auto) => "Auto",
        (false, UserMode::Work) => "Work",
        (false, UserMode::Gaming) => "Gaming",
        (false, UserMode::Idle) => "Idle",
        (false, UserMode::Maintenance) => "Maintenance",
        (false, UserMode::ForceCi) => "Force CI",
        (true, UserMode::Auto) => "自动",
        (true, UserMode::Work) => "工作",
        (true, UserMode::Gaming) => "游戏",
        (true, UserMode::Idle) => "空闲",
        (true, UserMode::Maintenance) => "维护",
        (true, UserMode::ForceCi) => "强制 CI",
    }
}

fn probe_label(language: LanguagePreference, id: &ProbeId) -> String {
    let known = match (language == LanguagePreference::ZhCn, id.as_str()) {
        (false, "user-activity") => Some("User activity"),
        (false, "steam-game") => Some("Steam game"),
        (false, "process-list") => Some("Process list"),
        (true, "user-activity") => Some("用户活动"),
        (true, "steam-game") => Some("Steam 游戏"),
        (true, "process-list") => Some("进程列表"),
        _ => None,
    };
    known.unwrap_or(id.as_str()).to_owned()
}

fn theme_label(language: LanguagePreference, theme: ThemePreference) -> &'static str {
    match (language == LanguagePreference::ZhCn, theme) {
        (false, ThemePreference::System) => "System",
        (false, ThemePreference::Light) => "Light",
        (false, ThemePreference::Dark) => "Dark",
        (true, ThemePreference::System) => "跟随系统",
        (true, ThemePreference::Light) => "浅色",
        (true, ThemePreference::Dark) => "深色",
    }
}

fn language_label(language: LanguagePreference, choice: LanguagePreference) -> &'static str {
    match (language == LanguagePreference::ZhCn, choice) {
        (false, LanguagePreference::System) => "System",
        (false, LanguagePreference::ZhCn) => "Simplified Chinese",
        (false, LanguagePreference::EnUs) => "English",
        (true, LanguagePreference::System) => "跟随系统",
        (true, LanguagePreference::ZhCn) => "简体中文",
        (true, LanguagePreference::EnUs) => "英语",
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, thread};

    use super::{
        render, NativeTrayEventLoop, TrayActionResult, TrayError, TrayMenuEntry, TrayMenuId,
        TrayUiUpdate,
    };
    use crate::{
        AdmissionDecision, AgentCommand, AgentHealth, AgentResponse, AgentSnapshot, AgentTransport,
        BuildProvenance, IpcRequest, IpcResponse, IpcResponseBody, IpcTransportError,
        LanguagePreference, LinkKind, LinkSnapshot, LinkState, NodeState, ProbeId,
        ProbeRuntimeState, ProbeSnapshot, ReasonCode, RunnerPhase, ThemePreference, UiPreferences,
        UserMode, ZenOverride, IPC_PROTOCOL_VERSION,
    };

    #[test]
    fn render_contains_frozen_information_architecture_and_non_color_semantics() {
        let rendered = render(&sample_snapshot());
        let ids = collect_ids(&rendered.entries);

        assert_eq!(rendered.icon_glyph.marker(), 'D');
        assert!(rendered.tooltip.contains("DRAINED"));
        for id in [
            TrayMenuId::Zen,
            TrayMenuId::Mode(UserMode::Auto),
            TrayMenuId::ProbeToggle(ProbeId::new("steam-game").unwrap()),
            TrayMenuId::Doctor,
            TrayMenuId::Theme(ThemePreference::Dark),
            TrayMenuId::Language(LanguagePreference::ZhCn),
            TrayMenuId::ExitAfterDrain,
        ] {
            assert!(ids.contains(&id.stable_id()), "missing {}", id.stable_id());
        }
    }

    #[test]
    fn language_and_theme_change_presentation_not_ids_or_admission() {
        let english_snapshot = sample_snapshot();
        let mut chinese_snapshot = english_snapshot.clone();
        chinese_snapshot.ui_preferences = UiPreferences {
            theme: ThemePreference::Dark,
            language: LanguagePreference::ZhCn,
        };
        let english = render(&english_snapshot);
        let chinese = render(&chinese_snapshot);

        assert_eq!(collect_ids(&english.entries), collect_ids(&chinese.entries));
        assert_ne!(english.entries, chinese.entries);
        assert_eq!(english_snapshot.admission, chinese_snapshot.admission);

        let transport = FakeTransport::new(AgentResponse::Accepted {
            snapshot: chinese_snapshot.clone(),
        });
        let mut tray = NativeTrayEventLoop::new(transport.clone());
        let result = tray
            .activate(&TrayMenuId::Theme(ThemePreference::Dark), &english_snapshot)
            .unwrap();
        assert!(matches!(result, TrayActionResult::Snapshot(_)));
        assert_eq!(
            transport.commands(),
            vec![AgentCommand::SetUiPreferences {
                ui_preferences: UiPreferences {
                    theme: ThemePreference::Dark,
                    language: LanguagePreference::System,
                },
            }]
        );
        assert_eq!(english_snapshot.admission, chinese_snapshot.admission);
    }

    #[test]
    fn mutation_from_a_non_ui_thread_is_rejected() {
        let snapshot = sample_snapshot();
        let tray = NativeTrayEventLoop::new(NopTransport);
        let result = thread::spawn(move || {
            let mut tray = tray;
            tray.apply(TrayUiUpdate::Snapshot(snapshot))
                .map(|_| ())
                .unwrap_err()
        })
        .join()
        .unwrap();
        assert!(matches!(result, TrayError::WrongUiThread));
    }

    #[derive(Clone)]
    struct FakeTransport {
        response: AgentResponse,
        commands: Rc<RefCell<Vec<AgentCommand>>>,
    }

    impl FakeTransport {
        fn new(response: AgentResponse) -> Self {
            Self {
                response,
                commands: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn commands(&self) -> Vec<AgentCommand> {
            self.commands.borrow().clone()
        }
    }

    impl AgentTransport for FakeTransport {
        fn call(&self, request: IpcRequest) -> Result<IpcResponse, IpcTransportError> {
            self.commands.borrow_mut().push(request.command);
            Ok(IpcResponse {
                protocol_version: IPC_PROTOCOL_VERSION,
                request_id: request.request_id,
                body: IpcResponseBody::Success(self.response.clone()),
            })
        }
    }

    struct NopTransport;

    impl AgentTransport for NopTransport {
        fn call(&self, _request: IpcRequest) -> Result<IpcResponse, IpcTransportError> {
            unreachable!("rendering does not contact the Agent")
        }
    }

    fn collect_ids(entries: &[TrayMenuEntry]) -> Vec<String> {
        let mut ids = Vec::new();
        for entry in entries {
            match entry {
                TrayMenuEntry::Item(item) => ids.push(item.id.stable_id()),
                TrayMenuEntry::Separator => {}
                TrayMenuEntry::Submenu { id, entries, .. } => {
                    ids.push(id.clone());
                    ids.extend(collect_ids(entries));
                }
            }
        }
        ids
    }

    fn sample_snapshot() -> AgentSnapshot {
        AgentSnapshot {
            schema_version: 1,
            build: BuildProvenance {
                version: "0.1.0-dev".to_owned(),
                commit: "0123456789abcdef".to_owned(),
                channel: "dev".to_owned(),
                target: "synthetic".to_owned(),
            },
            health: AgentHealth::Healthy,
            zen: ZenOverride::Disabled,
            user_mode: UserMode::Auto,
            node_state: NodeState::Drained,
            admission: AdmissionDecision {
                allow_new_work: false,
                desired_node_state: NodeState::Drained,
                reason_code: ReasonCode::new("awaiting-auto-policy").unwrap(),
                drain_requested: true,
            },
            runner_phase: RunnerPhase::Unknown,
            links: vec![LinkSnapshot {
                kind: LinkKind::GithubActions,
                state: LinkState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            probes: vec![ProbeSnapshot {
                id: ProbeId::new("steam-game").unwrap(),
                enabled: true,
                runtime_state: ProbeRuntimeState::Unknown,
                reason_code: Some(ReasonCode::new("not-observed").unwrap()),
            }],
            ui_preferences: UiPreferences::default(),
            start_on_login_preference: false,
            auto_idle_threshold_seconds: 300,
            update_checks_enabled: true,
        }
    }
}
