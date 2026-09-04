//! Ordinary-user development runtime for the persistent Windows Agent.
//!
//! This module deliberately requires an explicit development root. It never
//! selects an installed runtime location and its reconciler does not control an
//! official runner before H1.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    Icon, TrayIcon, TrayIconBuilder,
};

use crate::{
    ActivityWorkloadProbe, AdmissionControlSnapshot, AdmissionDecision, AgentCommand, AgentCore,
    AgentHealth, AgentObservation, AgentObserver, AgentReconciler, AgentResponse, AgentSnapshot,
    BuildProvenance, DesiredAdmissionState, DoctorCheck, DoctorReport, DoctorStatus,
    EffectiveLocale, EffectiveTheme, ExecutionIdentityEvidence, FileConfigStore, HardSafetyState,
    HostSnapshot, HostSource, IpcEndpoint, IpcServer, LanguagePreference, LinkKind, LinkSnapshot,
    LinkState, LocalAgentTransport, NativeTrayEventLoop, OfficialRunnerObserver, OwnershipEvidence,
    ProcessListProbe, ReasonCode, RunnerPhase, ThemePreference, TrayIconGlyph, TrayMenuEntry,
    TrayMenuId, TrayMenuItem, TrayRender, TrayUiUpdate, UserActivityProbe, WindowsHostSource,
    WindowsProcessSource, WindowsRunnerSource, WindowsSteamAppIdSource, WindowsUserActivitySource,
    WindowsUserSessionSupervisorAdapter,
};

/// Result persisted only inside the caller-owned development root, so automated
/// qualification can prove native initialization without treating a source
/// build as an installed runtime.
#[derive(Serialize)]
struct DevelopmentRuntimeEvidence {
    development_test_runtime: bool,
    native_tray_initialized: bool,
    native_icon_registered: bool,
    native_event_loop_alive: bool,
    pipe_server_reachable: bool,
    tray_refreshes: u64,
    theme: ThemePreference,
    effective_theme: EffectiveTheme,
    language: LanguagePreference,
    effective_locale: EffectiveLocale,
    menu_hints_enabled: bool,
}

/// Reconstructable runtime readiness facts used only to extend the development
/// Agent's doctor output. These facts never grant runner-control authority.
struct RuntimeReadiness {
    native_tray_ready: AtomicBool,
    runner_observer_configured: bool,
    supervisor_adapter_ready: bool,
}

impl RuntimeReadiness {
    fn from_runner_home(runner_home: Option<&Path>) -> Self {
        let runner_observer_configured = runner_home.is_some_and(Path::is_dir);
        let supervisor_adapter_ready = runner_home.is_some_and(|home| {
            WindowsUserSessionSupervisorAdapter::for_runner_home(home).readiness()
                == crate::WindowsSupervisorReadiness::Ready
        });
        Self {
            native_tray_ready: AtomicBool::new(false),
            runner_observer_configured,
            supervisor_adapter_ready,
        }
    }
}

type RuntimeCore = AgentCore<RuntimeObserver, NoRunnerControl, FileConfigStore>;

/// Starts the development-only Agent. `development_root` must be a caller-owned
/// sandbox path; no default is intentionally provided.
pub fn run_development_agent(
    development_root: PathBuf,
    runner_home: Option<PathBuf>,
    process_probe_names: Vec<String>,
) -> Result<(), String> {
    fs::create_dir_all(&development_root).map_err(|error| error.to_string())?;
    let store = FileConfigStore::new(development_root.join("config.json"));
    let readiness = Arc::new(RuntimeReadiness::from_runner_home(runner_home.as_deref()));
    let observer = RuntimeObserver::new(runner_home, process_probe_names);
    let mut core = AgentCore::new(observer, NoRunnerControl, store, build_provenance())
        .map_err(|error| error.to_string())?;
    let initial_snapshot = core
        .observe_decide_reconcile()
        .map_err(|error| error.to_string())?;
    write_runtime_stage(&development_root, "observation-complete")?;
    let core = Arc::new(Mutex::new(core));
    let exit_requested = Arc::new(AtomicBool::new(false));
    let endpoint = IpcEndpoint::for_current_user().map_err(|error| error.to_string())?;
    let server = IpcServer::bind(endpoint).map_err(|error| error.to_string())?;
    write_runtime_stage(&development_root, "pipe-bound")?;
    let (snapshot_sender, snapshot_receiver) = mpsc::channel();
    let pipe_thread = spawn_pipe_loop(
        server,
        Arc::clone(&core),
        Arc::clone(&exit_requested),
        Arc::clone(&readiness),
        snapshot_sender,
    );

    let result = run_native_tray_loop(
        &development_root,
        Arc::clone(&core),
        initial_snapshot,
        snapshot_receiver,
        Arc::clone(&exit_requested),
        readiness,
    );
    exit_requested.store(true, Ordering::Release);
    // Exit is requested through the same typed local IPC route. The pipe loop
    // returns after that request; never force-terminate a thread or runner.
    let _ = pipe_thread.join();
    result
}

fn build_provenance() -> BuildProvenance {
    BuildProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        commit: env!("RUNNERMESH_BUILD_COMMIT").to_owned(),
        channel: env!("RUNNERMESH_BUILD_CHANNEL").to_owned(),
        target: env!("RUNNERMESH_BUILD_TARGET").to_owned(),
    }
}

fn spawn_pipe_loop(
    server: IpcServer,
    core: Arc<Mutex<RuntimeCore>>,
    exit_requested: Arc<AtomicBool>,
    readiness: Arc<RuntimeReadiness>,
    snapshot_sender: Sender<AgentSnapshot>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !exit_requested.load(Ordering::Acquire) {
            let core = Arc::clone(&core);
            let exit_requested = Arc::clone(&exit_requested);
            let exit_for_after_serve = Arc::clone(&exit_requested);
            let readiness = Arc::clone(&readiness);
            let snapshot_sender = snapshot_sender.clone();
            let served = server.serve_once(move |command| {
                let mut core = core
                    .lock()
                    .map_err(|_| static_reason("agent-runtime-poisoned"))?;
                let response = if matches!(command, AgentCommand::ExitAfterDrain) {
                    // Before H1 this is an Agent-only development exit. It does
                    // not issue any real runner command or change a work root.
                    exit_requested.store(true, Ordering::Release);
                    AgentResponse::Accepted {
                        snapshot: core.snapshot(),
                    }
                } else {
                    let response = core
                        .handle_command(command)
                        .map_err(|_| static_reason("agent-runtime-command-failed"))?;
                    if let AgentResponse::Doctor(mut report) = response {
                        append_pre_h1_doctor_checks(&mut report, &core.snapshot(), &readiness);
                        AgentResponse::Doctor(report)
                    } else {
                        response
                    }
                };
                if let AgentResponse::Snapshot(snapshot) | AgentResponse::Accepted { snapshot } =
                    &response
                {
                    let _ = snapshot_sender.send(snapshot.clone());
                }
                Ok(response)
            });
            if served.is_err() && !exit_for_after_serve.load(Ordering::Acquire) {
                // A broken client is recoverable; the next loop iteration
                // recreates the one-shot Named Pipe server instance.
                thread::sleep(Duration::from_millis(10));
            }
        }
    })
}

fn run_native_tray_loop(
    development_root: &Path,
    core: Arc<Mutex<RuntimeCore>>,
    initial_snapshot: AgentSnapshot,
    snapshots: Receiver<AgentSnapshot>,
    exit_requested: Arc<AtomicBool>,
    readiness: Arc<RuntimeReadiness>,
) -> Result<(), String> {
    let transport = LocalAgentTransport::new(Duration::from_secs(2));
    let mut frontend = NativeTrayEventLoop::new(transport);
    let mut current_snapshot = initial_snapshot;
    let initial_render = frontend
        .apply(TrayUiUpdate::Snapshot(current_snapshot.clone()))
        .map_err(|error| error.to_string())?
        .clone();
    write_runtime_stage(development_root, "tray-starting")?;
    let mut native = NativeTrayBackend::new(
        development_root,
        &initial_render,
        current_snapshot.effective_ui_preferences.theme,
        current_snapshot.ui_preferences.theme != ThemePreference::System,
    )?;
    write_runtime_stage(development_root, "tray-backend-ready")?;
    readiness.native_tray_ready.store(true, Ordering::Release);
    // UISettings.ColorValuesChanged runs outside the tray ownership boundary.
    // The callback only marks a refresh pending; this loop resolves and
    // applies the owner-draw palette on the UI thread.
    let system_theme_monitor = crate::windows_preferences::SystemThemeChangeMonitor::new().ok();
    write_runtime_stage(development_root, "system-theme-monitor-ready")?;
    let mut refreshes = 1;
    write_runtime_evidence(development_root, &current_snapshot, refreshes)?;
    write_runtime_stage(development_root, "runtime-ready")?;
    write_hint_evidence(development_root, &mut native)?;
    let mut last_observation = Instant::now();

    while !exit_requested.load(Ordering::Acquire) {
        pump_windows_messages();
        if last_observation.elapsed() >= Duration::from_secs(1) {
            // Observation is reconstructed in the Agent Core on the normal
            // Observe -> Decide -> Reconcile path. It is independent of tray
            // rendering and has no pre-H1 runner-control backend.
            if let Ok(mut core) = core.lock() {
                if let Ok(snapshot) = core.observe_decide_reconcile() {
                    current_snapshot = snapshot;
                    let render = frontend
                        .apply(TrayUiUpdate::Snapshot(current_snapshot.clone()))
                        .map_err(|error| error.to_string())?;
                    native.apply(
                        render,
                        current_snapshot.effective_ui_preferences.theme,
                        current_snapshot.ui_preferences.theme != ThemePreference::System,
                    )?;
                    refreshes += 1;
                    write_runtime_evidence(development_root, &current_snapshot, refreshes)?;
                }
            }
            last_observation = Instant::now();
        }
        if native.take_dpi_refresh_request() {
            let render = frontend
                .current_render()
                .expect("the native tray always has a current presentation render");
            native.apply(
                render,
                current_snapshot.effective_ui_preferences.theme,
                current_snapshot.ui_preferences.theme != ThemePreference::System,
            )?;
            refreshes += 1;
            write_runtime_evidence(development_root, &current_snapshot, refreshes)?;
        }
        maybe_run_hint_exercise(development_root, &native)?;
        write_hint_evidence(development_root, &mut native)?;

        for (request_name, enabled) in [
            ("development-menu-hints-disable.request", false),
            ("development-menu-hints-enable.request", true),
        ] {
            if let Some(snapshot) = maybe_toggle_menu_hints_for_development(
                development_root,
                &mut frontend,
                &current_snapshot,
                request_name,
                enabled,
            )? {
                current_snapshot = snapshot;
                let render = frontend
                    .current_render()
                    .expect("a successful development tray action updates the render");
                native.apply(
                    render,
                    current_snapshot.effective_ui_preferences.theme,
                    current_snapshot.ui_preferences.theme != ThemePreference::System,
                )?;
                refreshes += 1;
                write_runtime_evidence(development_root, &current_snapshot, refreshes)?;
            }
        }

        if current_snapshot.ui_preferences.theme == ThemePreference::System
            && system_theme_monitor
                .as_ref()
                .is_some_and(crate::windows_preferences::SystemThemeChangeMonitor::take_change)
        {
            let observed = crate::windows_preferences::observe_system_preferences()?;
            current_snapshot.effective_ui_preferences.theme = observed.theme;
            let render = frontend
                .apply(TrayUiUpdate::Snapshot(current_snapshot.clone()))
                .map_err(|error| error.to_string())?;
            native.apply(
                render,
                current_snapshot.effective_ui_preferences.theme,
                current_snapshot.ui_preferences.theme != ThemePreference::System,
            )?;
            refreshes += 1;
            write_runtime_evidence(development_root, &current_snapshot, refreshes)?;
        }

        while let Ok(snapshot) = snapshots.try_recv() {
            current_snapshot = snapshot;
            let render = frontend
                .apply(TrayUiUpdate::Snapshot(current_snapshot.clone()))
                .map_err(|error| error.to_string())?;
            native.apply(
                render,
                current_snapshot.effective_ui_preferences.theme,
                current_snapshot.ui_preferences.theme != ThemePreference::System,
            )?;
            refreshes += 1;
            write_runtime_evidence(development_root, &current_snapshot, refreshes)?;
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let Some(action) = native.action(event.id.as_ref()).cloned() else {
                continue;
            };
            match frontend
                .activate(&action, &current_snapshot)
                .map_err(|error| error.to_string())?
            {
                crate::TrayActionResult::Snapshot(snapshot) => {
                    current_snapshot = *snapshot;
                    let render = frontend
                        .current_render()
                        .expect("a successful tray action updates the render");
                    native.apply(
                        render,
                        current_snapshot.effective_ui_preferences.theme,
                        current_snapshot.ui_preferences.theme != ThemePreference::System,
                    )?;
                    refreshes += 1;
                    write_runtime_evidence(development_root, &current_snapshot, refreshes)?;
                }
                crate::TrayActionResult::Doctor(_)
                | crate::TrayActionResult::Rejected(_)
                | crate::TrayActionResult::NoOp => {}
            }
        }

        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn write_runtime_evidence(
    development_root: &Path,
    snapshot: &AgentSnapshot,
    tray_refreshes: u64,
) -> Result<(), String> {
    let evidence = DevelopmentRuntimeEvidence {
        development_test_runtime: true,
        native_tray_initialized: true,
        native_icon_registered: true,
        native_event_loop_alive: true,
        pipe_server_reachable: true,
        tray_refreshes,
        theme: snapshot.ui_preferences.theme,
        effective_theme: snapshot.effective_ui_preferences.theme,
        language: snapshot.ui_preferences.language,
        effective_locale: snapshot.effective_ui_preferences.locale,
        menu_hints_enabled: snapshot.ui_preferences.menu_hints_enabled,
    };
    let bytes = serde_json::to_vec_pretty(&evidence).map_err(|error| error.to_string())?;
    fs::write(development_root.join("runtime-evidence.json"), bytes)
        .map_err(|error| error.to_string())
}

fn write_runtime_stage(development_root: &Path, stage: &str) -> Result<(), String> {
    fs::write(development_root.join("runtime-stage.txt"), stage).map_err(|error| error.to_string())
}

fn write_hint_evidence(
    development_root: &Path,
    native: &mut NativeTrayBackend,
) -> Result<(), String> {
    let Some(evidence) = native.take_hint_evidence() else {
        return Ok(());
    };
    let bytes = serde_json::to_vec_pretty(&evidence).map_err(|error| error.to_string())?;
    fs::write(development_root.join("hint-evidence.json"), bytes).map_err(|error| error.to_string())
}

fn maybe_run_hint_exercise(
    development_root: &Path,
    native: &NativeTrayBackend,
) -> Result<(), String> {
    let request = development_root.join("development-hint-exercise.request");
    if !request.exists() {
        return Ok(());
    }
    fs::remove_file(&request).map_err(|error| error.to_string())?;
    unsafe {
        crate::windows_tray_theme::queue_development_hint_exercise(
            native.tray.window_handle(),
            &native.theme,
        )
    }
}

/// Drives the normal stable tray action through the real local Agent IPC only
/// when an isolated development root requests a presentation-evidence toggle.
/// It is intentionally unavailable to installed/runtime profiles.
fn maybe_toggle_menu_hints_for_development(
    development_root: &Path,
    frontend: &mut NativeTrayEventLoop<LocalAgentTransport>,
    current_snapshot: &AgentSnapshot,
    request_name: &str,
    enabled: bool,
) -> Result<Option<AgentSnapshot>, String> {
    let request = development_root.join(request_name);
    if !request.exists() {
        return Ok(None);
    }
    fs::remove_file(&request).map_err(|error| error.to_string())?;
    if current_snapshot.ui_preferences.menu_hints_enabled == enabled {
        return Ok(None);
    }
    match frontend
        .activate(&TrayMenuId::MenuHints, current_snapshot)
        .map_err(|error| error.to_string())?
    {
        crate::TrayActionResult::Snapshot(snapshot) => Ok(Some(*snapshot)),
        crate::TrayActionResult::Doctor(_)
        | crate::TrayActionResult::Rejected(_)
        | crate::TrayActionResult::NoOp => {
            Err("development menu-hints action did not produce an Agent snapshot".to_owned())
        }
    }
}

struct NativeTrayBackend {
    theme: crate::windows_tray_theme::ThemedMenu,
    tray: TrayIcon,
    actions: HashMap<String, TrayMenuId>,
    hints: Box<crate::windows_tray_theme::MenuHintTooltip>,
}

impl NativeTrayBackend {
    fn new(
        development_root: &Path,
        render: &TrayRender,
        theme: EffectiveTheme,
        explicit_theme: bool,
    ) -> Result<Self, String> {
        let (menu, actions) = build_native_menu(render)?;
        write_runtime_stage(development_root, "native-menu-built")?;
        let tray = TrayIconBuilder::new()
            .with_id("runnermesh-agent")
            .with_tooltip(&render.tooltip)
            .build()
            .map_err(|error| error.to_string())?;
        write_runtime_stage(development_root, "native-icon-built")?;
        let themed_menu = unsafe {
            crate::windows_tray_theme::theme_popup_menu(&menu, render, theme, tray.window_handle())?
        };
        tray.set_menu(Some(Box::new(menu)));
        tray.set_icon(Some(icon_for(render.icon_glyph, unsafe {
            crate::windows_tray_theme::small_icon_size_for_window(tray.window_handle())
        })?))
        .map_err(|error| error.to_string())?;
        write_runtime_stage(development_root, "native-menu-themed")?;
        tray.set_show_menu_on_left_click(true);
        write_runtime_stage(development_root, "native-menu-click-configured")?;
        // `tray-icon` guarantees this HWND for the icon lifetime; construction
        // and all later tray mutation occur on this UI thread.
        let mut hints = unsafe {
            crate::windows_tray_theme::MenuHintTooltip::new(tray.window_handle(), |stage| {
                write_runtime_stage(development_root, stage)
            })?
        };
        hints.update_menu_presentation(&themed_menu, render, explicit_theme);
        write_runtime_stage(development_root, "native-hints-created")?;
        unsafe {
            crate::windows_tray_theme::install_owner_draw_hook(
                tray.window_handle(),
                hints.as_mut() as *mut _,
            )?;
        }
        write_runtime_stage(development_root, "native-hook-installed")?;
        Ok(Self {
            theme: themed_menu,
            tray,
            actions,
            hints,
        })
    }

    fn apply(
        &mut self,
        render: &TrayRender,
        theme: EffectiveTheme,
        explicit_theme: bool,
    ) -> Result<(), String> {
        unsafe {
            self.hints
                .dismiss_for_menu_rebuild(!render.menu_hints_enabled);
        }
        let (menu, actions) = build_native_menu(render)?;
        let themed_menu = unsafe {
            crate::windows_tray_theme::theme_popup_menu(
                &menu,
                render,
                theme,
                self.tray.window_handle(),
            )?
        };
        self.tray.set_menu(Some(Box::new(menu)));
        // `set_menu` synchronously detaches and drops the previous muda menu;
        // only then may its owner-draw data and background brushes be retired.
        self.theme = themed_menu;
        self.hints
            .update_menu_presentation(&self.theme, render, explicit_theme);
        self.tray
            .set_tooltip(Some(&render.tooltip))
            .map_err(|error| error.to_string())?;
        self.tray
            .set_icon(Some(icon_for(render.icon_glyph, unsafe {
                crate::windows_tray_theme::small_icon_size_for_window(self.tray.window_handle())
            })?))
            .map_err(|error| error.to_string())?;
        self.actions = actions;
        Ok(())
    }

    fn action(&self, id: &str) -> Option<&TrayMenuId> {
        self.actions.get(id)
    }

    fn take_hint_evidence(&mut self) -> Option<crate::windows_tray_theme::NativeHintEvidence> {
        self.hints.take_evidence()
    }

    fn take_dpi_refresh_request(&mut self) -> bool {
        self.hints.take_dpi_refresh_request()
    }
}

impl Drop for NativeTrayBackend {
    fn drop(&mut self) {
        // The UI loop is exiting on its owner thread. Detach the menu before
        // the backing owner-draw records are dropped, then remove our distinct
        // common-controls subclass before tray-icon destroys its hidden window.
        self.tray.set_menu(None);
        unsafe {
            crate::windows_tray_theme::remove_owner_draw_hook(self.tray.window_handle());
            self.hints.dispose();
        }
    }
}

fn build_native_menu(render: &TrayRender) -> Result<(Menu, HashMap<String, TrayMenuId>), String> {
    let menu = Menu::with_id("runnermesh.root");
    let mut actions = HashMap::new();
    for entry in &render.entries {
        append_to_menu(&menu, entry, &mut actions)?;
    }
    Ok((menu, actions))
}

fn append_to_menu(
    menu: &Menu,
    entry: &TrayMenuEntry,
    actions: &mut HashMap<String, TrayMenuId>,
) -> Result<(), String> {
    match entry {
        TrayMenuEntry::Separator => menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|error| error.to_string()),
        TrayMenuEntry::Item(item) => append_item_to_menu(menu, item, actions),
        TrayMenuEntry::Submenu { id, label, entries } => {
            let submenu = Submenu::with_id(id, label, true);
            for child in entries {
                append_to_submenu(&submenu, child, actions)?;
            }
            menu.append(&submenu).map_err(|error| error.to_string())
        }
    }
}

fn append_to_submenu(
    submenu: &Submenu,
    entry: &TrayMenuEntry,
    actions: &mut HashMap<String, TrayMenuId>,
) -> Result<(), String> {
    match entry {
        TrayMenuEntry::Separator => submenu
            .append(&PredefinedMenuItem::separator())
            .map_err(|error| error.to_string()),
        TrayMenuEntry::Item(item) => append_item_to_submenu(submenu, item, actions),
        TrayMenuEntry::Submenu { id, label, entries } => {
            let child = Submenu::with_id(id, label, true);
            for entry in entries {
                append_to_submenu(&child, entry, actions)?;
            }
            submenu.append(&child).map_err(|error| error.to_string())
        }
    }
}

fn append_item_to_menu(
    menu: &Menu,
    item: &TrayMenuItem,
    actions: &mut HashMap<String, TrayMenuId>,
) -> Result<(), String> {
    if let Some(checked) = item.checked {
        let native = CheckMenuItem::with_id(
            item.id.stable_id(),
            &item.label,
            item.enabled,
            checked,
            None,
        );
        menu.append(&native).map_err(|error| error.to_string())?;
    } else {
        let native = MenuItem::with_id(item.id.stable_id(), &item.label, item.enabled, None);
        menu.append(&native).map_err(|error| error.to_string())?;
    }
    register_action(item, actions);
    Ok(())
}

fn append_item_to_submenu(
    submenu: &Submenu,
    item: &TrayMenuItem,
    actions: &mut HashMap<String, TrayMenuId>,
) -> Result<(), String> {
    if let Some(checked) = item.checked {
        let native = CheckMenuItem::with_id(
            item.id.stable_id(),
            &item.label,
            item.enabled,
            checked,
            None,
        );
        submenu.append(&native).map_err(|error| error.to_string())?;
    } else {
        let native = MenuItem::with_id(item.id.stable_id(), &item.label, item.enabled, None);
        submenu.append(&native).map_err(|error| error.to_string())?;
    }
    register_action(item, actions);
    Ok(())
}

fn register_action(item: &TrayMenuItem, actions: &mut HashMap<String, TrayMenuId>) {
    if item.enabled {
        actions.insert(item.id.stable_id(), item.id.clone());
    }
}

fn icon_for(glyph: TrayIconGlyph, size: u32) -> Result<Icon, String> {
    let size = size.clamp(16, 256) as usize;
    let inset = (size / 8).max(1);
    let stroke = (size / 12).max(1);
    let middle = size / 2;
    let mut rgba = vec![0_u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let visible = match glyph {
                TrayIconGlyph::Full => {
                    (x >= inset && x < inset + stroke && y >= inset && y < size - inset)
                        || (x >= size - inset - stroke
                            && x < size - inset
                            && y >= inset
                            && y < size - inset)
                        || (y >= inset && y < inset + stroke && x >= inset && x < size - inset)
                        || (y >= size - inset - stroke
                            && y < size - inset
                            && x >= inset
                            && x < size - inset)
                }
                TrayIconGlyph::Throttled => {
                    x % (stroke * 3).max(2) < stroke && y >= inset && y < size.saturating_sub(inset)
                }
                TrayIconGlyph::Drained => {
                    y.abs_diff(middle) < stroke && x >= inset && x < size.saturating_sub(inset)
                }
                TrayIconGlyph::Offline => {
                    x.abs_diff(y) < stroke || x.saturating_add(y).abs_diff(size - 1) < stroke
                }
            };
            if visible {
                let index = (y * size + x) * 4;
                rgba[index..index + 4].copy_from_slice(&[20, 20, 20, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, size as u32, size as u32).map_err(|error| error.to_string())
}

fn pump_windows_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

struct RuntimeObserver {
    host: WindowsHostSource,
    user_activity: UserActivityProbe<WindowsUserActivitySource>,
    steam: crate::SteamGameProbe<WindowsSteamAppIdSource>,
    process_list: ProcessListProbe<WindowsProcessSource>,
    runner_home: Option<PathBuf>,
}

impl RuntimeObserver {
    fn new(runner_home: Option<PathBuf>, process_probe_names: Vec<String>) -> Self {
        Self {
            host: WindowsHostSource::default(),
            user_activity: UserActivityProbe::new(WindowsUserActivitySource, 300),
            steam: crate::SteamGameProbe::new(WindowsSteamAppIdSource),
            process_list: ProcessListProbe::new(WindowsProcessSource, process_probe_names),
            runner_home,
        }
    }
}

impl AgentObserver for RuntimeObserver {
    fn observe(&mut self) -> Result<AgentObservation, String> {
        let host = HostSnapshot::from_evidence(self.host.collect());
        let (runner_phase, execution_identity, work_root, links) = match &self.runner_home {
            Some(home) => {
                let observed =
                    OfficialRunnerObserver::new(WindowsRunnerSource::new(home)).observe();
                (
                    observed.phase,
                    observed.execution_identity,
                    observed.work_root,
                    vec![observed.github_link],
                )
            }
            None => (
                RunnerPhase::Unknown,
                ExecutionIdentityEvidence::Unknown,
                OwnershipEvidence::Unknown,
                vec![LinkSnapshot {
                    kind: LinkKind::GithubActions,
                    state: LinkState::NotConfigured,
                    reason_code: Some(static_reason("runner-home-not-configured")),
                }],
            ),
        };
        Ok(AgentObservation {
            health: host.health.health,
            health_reason_code: Some(host.health.reason_code),
            hard_safety: HardSafetyState::Clear,
            runner_phase,
            execution_identity,
            work_root,
            admission_control: AdmissionControlSnapshot::not_configured(
                DesiredAdmissionState::Drained,
            ),
            links,
            probes: vec![
                self.user_activity.observe(),
                self.steam.observe(),
                self.process_list.observe(),
            ],
            system_preferences: crate::windows_preferences::observe_system_preferences()?,
        })
    }
}

struct NoRunnerControl;

impl AgentReconciler for NoRunnerControl {
    fn reconcile(
        &mut self,
        decision: &AdmissionDecision,
        observation: &AgentObservation,
    ) -> Result<AdmissionControlSnapshot, String> {
        // This explicit pre-H1 reconciler has no process-control backend.
        Ok(observation
            .admission_control
            .clone()
            .with_desired(DesiredAdmissionState::from_decision(decision)))
    }
}

fn append_pre_h1_doctor_checks(
    report: &mut DoctorReport,
    snapshot: &AgentSnapshot,
    readiness: &RuntimeReadiness,
) {
    let check = |id: &'static str, status, reason: Option<&'static str>| DoctorCheck {
        id: static_reason(id),
        status,
        reason_code: reason.map(static_reason),
    };
    let probes_ready = ["user-activity", "steam-game", "process-list"]
        .into_iter()
        .all(|id| snapshot.probes.iter().any(|probe| probe.id.as_str() == id));
    let host_status = if snapshot.health == AgentHealth::Healthy {
        DoctorStatus::Pass
    } else {
        DoctorStatus::Warn
    };
    let host_reason = (host_status != DoctorStatus::Pass).then_some("host-observation-degraded");
    let runner_status = if readiness.runner_observer_configured {
        DoctorStatus::Pass
    } else {
        DoctorStatus::Warn
    };
    let supervisor_status = if readiness.supervisor_adapter_ready {
        DoctorStatus::Pass
    } else {
        DoctorStatus::Warn
    };
    let pre_h1_ready = readiness.native_tray_ready.load(Ordering::Acquire)
        && readiness.runner_observer_configured
        && readiness.supervisor_adapter_ready
        && probes_ready;

    report.checks.extend([
        check("agent-runtime", DoctorStatus::Pass, None),
        check(
            "native-tray",
            if readiness.native_tray_ready.load(Ordering::Acquire) {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Warn
            },
            (!readiness.native_tray_ready.load(Ordering::Acquire))
                .then_some("native-tray-starting"),
        ),
        check("local-ipc", DoctorStatus::Pass, None),
        check(
            "probe-system",
            if probes_ready {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Warn
            },
            (!probes_ready).then_some("probe-set-incomplete"),
        ),
        check("host-observation", host_status, host_reason),
        check(
            "runner-observer",
            runner_status,
            (!readiness.runner_observer_configured).then_some("runner-home-not-configured"),
        ),
        check(
            "supervisor-adapter",
            supervisor_status,
            (!readiness.supervisor_adapter_ready).then_some("supervisor-launch-context-not-ready"),
        ),
        check(
            "work-root-safety",
            DoctorStatus::Pass,
            Some("work-root-verification-required-at-h1"),
        ),
        check(
            "pre-h1-runtime-ready",
            if pre_h1_ready {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Warn
            },
            (!pre_h1_ready).then_some("pre-h1-runtime-incomplete"),
        ),
        check(
            "real-runner-drain",
            DoctorStatus::Warn,
            Some("h1-real-runner-lifecycle-required"),
        ),
    ]);
}

fn static_reason(value: &'static str) -> ReasonCode {
    ReasonCode::new(value).expect("static runtime reason codes must be valid")
}

#[cfg(test)]
mod tests {
    use super::icon_for;
    use crate::TrayIconGlyph;

    #[test]
    fn deterministic_icons_cover_every_capacity_glyph() {
        for glyph in [
            TrayIconGlyph::Full,
            TrayIconGlyph::Throttled,
            TrayIconGlyph::Drained,
            TrayIconGlyph::Offline,
        ] {
            assert!(icon_for(glyph, 16).is_ok());
            assert!(icon_for(glyph, 40).is_ok());
        }
    }

    #[test]
    fn agent_manifest_declares_common_controls_and_per_monitor_v2() {
        let manifest = include_str!("../resources/runnermesh-agent.manifest");
        assert!(manifest.contains("Microsoft.Windows.Common-Controls"));
        assert!(manifest.contains("PerMonitorV2, PerMonitor"));
        assert!(manifest.contains("true/pm"));
    }
}
