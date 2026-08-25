//! Windows-only owner-drawn presentation for the notification-area popup.
//!
//! `muda` remains the logical menu and command-ID provider. This module changes
//! only the visual presentation of its popup `HMENU` using the documented
//! owner-draw menu contract. It deliberately has no access to Agent state,
//! policy, runner control, or probe logic.

use std::{
    collections::HashMap,
    mem::{offset_of, size_of},
    sync::OnceLock,
    time::{Duration, Instant},
};

use serde::Serialize;

use tray_icon::menu::{ContextMenu, Menu};
use windows_sys::Win32::{
    Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        CreateFontIndirectW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetMonitorInfoW,
        GetSysColor, MonitorFromRect, SelectObject, SetBkMode, SetTextColor, COLOR_GRAYTEXT,
        COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_MENU, COLOR_MENUTEXT, DT_LEFT, DT_NOPREFIX,
        DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HBRUSH, HFONT, MONITORINFO,
        MONITOR_DEFAULTTONEAREST, TRANSPARENT,
    },
    System::{
        LibraryLoader::{GetModuleFileNameW, GetModuleHandleW, GetProcAddress},
        WindowsProgramming::MulDiv,
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        Controls::{
            InitCommonControlsEx, IsAppThemed, IsThemeActive, CCM_GETVERSION, CDDS_PREPAINT,
            CDRF_DODEFAULT, CDRF_SKIPDEFAULT, DRAWITEMSTRUCT, ICC_WIN95_CLASSES,
            INITCOMMONCONTROLSEX, MEASUREITEMSTRUCT, NMTTCUSTOMDRAW, NM_CUSTOMDRAW, ODS_CHECKED,
            ODS_DISABLED, ODS_SELECTED, ODT_MENU, TOOLTIPS_CLASSW, TTF_ABSOLUTE, TTF_TRACK,
            TTM_ADDTOOLW, TTM_DELTOOLW, TTM_GETBUBBLESIZE, TTM_GETMARGIN, TTM_GETTOOLCOUNT,
            TTM_GETTOOLINFOW, TTM_SETMAXTIPWIDTH, TTM_TRACKACTIVATE, TTM_TRACKPOSITION,
            TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
        },
        HiDpi::{
            AreDpiAwarenessContextsEqual, GetAwarenessFromDpiAwarenessContext, GetDpiForWindow,
            GetSystemMetricsForDpi, GetThreadDpiAwarenessContext, GetWindowDpiAwarenessContext,
            SystemParametersInfoForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            DPI_AWARENESS_PER_MONITOR_AWARE,
        },
        Shell::{
            DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass, DLLGETVERSIONPROC,
            DLLVERSIONINFO,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos,
            GetMenuItemCount, GetMenuItemInfoW, GetMenuItemRect, GetSubMenu, GetWindowLongPtrW,
            GetWindowRect, IsWindow, IsWindowVisible, KillTimer, PostMessageW, RegisterClassW,
            SendMessageW, SetMenuInfo, SetMenuItemInfoW, SetTimer, SetWindowLongPtrW, SetWindowPos,
            SystemParametersInfoW, CREATESTRUCTW, GWLP_USERDATA, GWL_EXSTYLE, HMENU, HWND_TOPMOST,
            MENUINFO, MENUITEMINFOW, MFT_OWNERDRAW, MFT_SEPARATOR, MF_POPUP, MIIM_DATA, MIIM_FTYPE,
            MIIM_ID, MIM_BACKGROUND, NONCLIENTMETRICSW, SM_CXMENUCHECK, SM_CXSMICON, SM_CYMENU,
            SPI_GETHIGHCONTRAST, SPI_GETNONCLIENTMETRICS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            WM_APP, WM_DPICHANGED, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUSELECT,
            WM_NCCREATE, WM_NOTIFY, WM_TIMER, WM_UNINITMENUPOPUP, WNDCLASSW, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
};

use crate::{localized_menu_hint, EffectiveTheme, TrayHelpKey, TrayMenuEntry, TrayRender};

const THEME_SUBCLASS_ID: usize = 0x5255_4E4E_4552_4D45;
const ITEM_HEIGHT_LOGICAL: i32 = 26;
const SEPARATOR_HEIGHT_LOGICAL: i32 = 9;
const ITEM_WIDTH_LOGICAL: i32 = 264;
const CHECK_GUTTER_LOGICAL: i32 = 30;
const SUBMENU_GUTTER_LOGICAL: i32 = 22;
const ITEM_INSET_LOGICAL: i32 = 10;
const HINT_TIMER_ID: usize = 0x524d_0603;
const HINT_EXERCISE_CLOSE_TIMER_ID: usize = 0x524d_0604;
const HINT_DELAY_MS: u32 = 500;
const HINT_HOST_INIT_NATIVE_TOOLTIP: u32 = WM_APP + 0x601;
const HINT_HOST_SELECTION_CHANGED: u32 = WM_APP + 0x602;
const HINT_HOST_DISMISS: u32 = WM_APP + 0x603;
const HINT_VIRTUAL_TOOL_ID: usize = 1;
const HINT_HOST_CLASS_NAME: &str = "RunnerMesh.NativeHintHost";
static HINT_HOST_CLASS: OnceLock<Result<(), String>> = OnceLock::new();

fn scale_logical_px(logical_96dpi: i32, dpi: u32) -> i32 {
    unsafe { MulDiv(logical_96dpi, dpi.max(96) as i32, 96) }
}

unsafe fn menu_dpi_resources(window: HWND) -> Result<MenuDpiResources, String> {
    let dpi = GetDpiForWindow(window).max(96);
    let mut nonclient = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    if SystemParametersInfoForDpi(
        SPI_GETNONCLIENTMETRICS,
        nonclient.cbSize,
        (&mut nonclient as *mut NONCLIENTMETRICSW).cast(),
        0,
        dpi,
    ) == 0
    {
        return Err("could not obtain DPI-correct Windows menu font metrics".to_owned());
    }
    let font = CreateFontIndirectW(&nonclient.lfMenuFont);
    if font.is_null() {
        return Err("could not create the DPI-correct Windows menu font".to_owned());
    }
    let native_menu_height = GetSystemMetricsForDpi(SM_CYMENU, dpi).max(1);
    let native_check_width = GetSystemMetricsForDpi(SM_CXMENUCHECK, dpi).max(1);
    let metrics = MenuDpiMetrics {
        item_height: scale_logical_px(ITEM_HEIGHT_LOGICAL, dpi).max(native_menu_height) as u32,
        separator_height: scale_logical_px(SEPARATOR_HEIGHT_LOGICAL, dpi).max(1) as u32,
        item_width: scale_logical_px(ITEM_WIDTH_LOGICAL, dpi).max(native_check_width) as u32,
        check_gutter: scale_logical_px(CHECK_GUTTER_LOGICAL, dpi).max(native_check_width),
        submenu_gutter: scale_logical_px(SUBMENU_GUTTER_LOGICAL, dpi).max(native_check_width),
        item_inset: scale_logical_px(ITEM_INSET_LOGICAL, dpi).max(1),
    };
    Ok(MenuDpiResources { dpi, metrics, font })
}

/// Returns the current small-icon pixel size for a tray window. The generated
/// glyph is procedural, so it is rendered directly at this size rather than
/// allowing a 96-DPI bitmap to be enlarged by the shell.
/// # Safety
///
/// `window` must be a live UI-thread window handle.
pub unsafe fn small_icon_size_for_window(window: HWND) -> u32 {
    let dpi = GetDpiForWindow(window).max(96);
    GetSystemMetricsForDpi(SM_CXSMICON, dpi).clamp(16, 256) as u32
}

unsafe fn is_per_monitor_v2_context(
    context: windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT,
) -> bool {
    GetAwarenessFromDpiAwarenessContext(context) == DPI_AWARENESS_PER_MONITOR_AWARE
        && AreDpiAwarenessContextsEqual(context, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct NativeHintEvidence {
    pub init_menu_popup_count: u64,
    pub menu_select_count: u64,
    pub uninit_menu_popup_count: u64,
    pub menu_close_count: u64,
    pub hint_key_resolution_count: u64,
    pub hint_show_request_count: u64,
    pub tooltip_activation_request_count: u64,
    pub tooltip_deactivation_count: u64,
    pub hint_backend_created: bool,
    pub hint_host_created: bool,
    pub hint_host_init_message_received: bool,
    pub tooltip_common_controls_v6_manifest: bool,
    pub tooltip_common_controls_version: u32,
    pub comctl_before_tooltip_loaded: bool,
    pub comctl_after_tooltip_loaded: bool,
    pub comctl_dll_major: u32,
    pub comctl_dll_minor: u32,
    pub comctl_module_path_class: String,
    pub comctl6_active: bool,
    pub app_themed: bool,
    pub theme_active: bool,
    pub tooltip_toolinfo_cb_size: u32,
    pub tooltip_layout_size: usize,
    pub tooltip_offset_cb_size: usize,
    pub tooltip_offset_flags: usize,
    pub tooltip_offset_hwnd: usize,
    pub tooltip_offset_uid: usize,
    pub tooltip_offset_rect: usize,
    pub tooltip_offset_hinst: usize,
    pub tooltip_offset_text: usize,
    pub tooltip_offset_lparam: usize,
    pub tooltip_offset_reserved: usize,
    pub tooltip_v1_size: u32,
    pub tooltip_v2_size: u32,
    pub tooltip_v3_size: u32,
    pub tooltip_matrix_v3_add: bool,
    pub tooltip_matrix_v3_get: bool,
    pub tooltip_matrix_v2_add: bool,
    pub tooltip_matrix_v2_get: bool,
    pub tooltip_matrix_v1_add: bool,
    pub tooltip_matrix_v1_get: bool,
    pub tooltip_window_created: bool,
    pub tooltip_window_valid_before_registration: bool,
    pub tooltip_tool_host_is_hint_host: bool,
    pub tooltip_child_window_valid: bool,
    pub tooltip_virtual_flags: u32,
    pub tooltip_child_flags: u32,
    pub tooltip_tool_count_before: isize,
    pub tooltip_tool_count_after: isize,
    pub tooltip_add_return: bool,
    pub tooltip_gettoolinfo_return: bool,
    pub tooltip_identity_virtual_numeric: bool,
    pub tooltip_identity_child_hwnd: bool,
    pub tooltip_virtual_add_return: bool,
    pub tooltip_virtual_gettoolinfo_return: bool,
    pub tooltip_child_add_return: bool,
    pub tooltip_child_gettoolinfo_return: bool,
    pub tooltip_tool_registered: bool,
    pub tooltip_text_readback: bool,
    pub tooltip_nonactivating: bool,
    pub tooltip_font_system_native: bool,
    pub tooltip_max_width_configured: bool,
    pub tooltip_nm_customdraw_received: bool,
    pub tooltip_overlaps_active_menu: bool,
    pub hint_text_nonempty: bool,
    pub hint_window_valid: bool,
    pub hint_window_visible: bool,
    pub hint_window_topmost: bool,
    pub hint_window_on_monitor: bool,
    pub active_menu_item_rect_valid: bool,
    pub tooltip_bubble_size_valid: bool,
    pub monitor_work_area_valid: bool,
    pub tooltip_rect_inside_work_area: bool,
    pub tooltip_placement_direction: String,
    pub tooltip_placement_clamped: bool,
    pub tooltip_used_menu_item_anchor: bool,
    pub thread_dpi_awareness: i32,
    pub per_monitor_v2_thread: bool,
    pub per_monitor_v2_tray_owner: bool,
    pub per_monitor_v2_hint_host: bool,
    pub per_monitor_v2_tooltip: bool,
    pub tray_owner_dpi: u32,
    pub hint_host_dpi: u32,
    pub tooltip_dpi: u32,
    pub tray_icon_size_px: u32,
    pub menu_font_dpi: u32,
    pub menu_metrics_dpi: u32,
    pub dpi_change_count: u64,
    pub no_bitmap_scale_fallback: bool,
    pub hint_visible_lifetime: bool,
    pub hint_hide_on_menu_close: bool,
    pub hint_hide_when_disabled: bool,
    pub hint_paint_count: u64,
    pub zen_hint_key_resolved: bool,
    pub mode_submenu_hint_key_resolved: bool,
    pub mode_child_hint_key_resolved: bool,
    pub probes_submenu_hint_key_resolved: bool,
    pub probe_child_hint_key_resolved: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrayThemePalette {
    surface: COLORREF,
    text: COLORREF,
    selected_surface: COLORREF,
    selected_text: COLORREF,
    disabled_text: COLORREF,
    separator: COLORREF,
}

impl TrayThemePalette {
    fn for_effective(theme: EffectiveTheme) -> Self {
        match theme {
            EffectiveTheme::Light => Self {
                surface: rgb(250, 250, 250),
                text: rgb(32, 32, 32),
                selected_surface: rgb(0, 95, 184),
                selected_text: rgb(255, 255, 255),
                disabled_text: rgb(128, 128, 128),
                separator: rgb(218, 218, 218),
            },
            EffectiveTheme::Dark => Self {
                surface: rgb(36, 36, 36),
                text: rgb(242, 242, 242),
                selected_surface: rgb(65, 105, 165),
                selected_text: rgb(255, 255, 255),
                disabled_text: rgb(150, 150, 150),
                separator: rgb(86, 86, 86),
            },
            EffectiveTheme::SystemAccessible => Self {
                surface: unsafe { GetSysColor(COLOR_MENU) },
                text: unsafe { GetSysColor(COLOR_MENUTEXT) },
                selected_surface: unsafe { GetSysColor(COLOR_HIGHLIGHT) },
                selected_text: unsafe { GetSysColor(COLOR_HIGHLIGHTTEXT) },
                disabled_text: unsafe { GetSysColor(COLOR_GRAYTEXT) },
                separator: unsafe { GetSysColor(COLOR_GRAYTEXT) },
            },
        }
    }
}

/// Keeps every owner-draw item pointer and background brush alive for exactly
/// the lifetime of the matching `muda` popup menu. The tray UI thread replaces
/// this object only after its synchronous popup tracking has returned.
pub struct ThemedMenu {
    // Every `dwItemData` field receives a pointer into this collection. A Box
    // preserves that address across Vec growth and until the muda popup is
    // detached, which is required by the Win32 owner-draw lifetime contract.
    #[allow(clippy::vec_box)]
    items: Vec<Box<OwnerDrawItem>>,
    brushes: Vec<HBRUSH>,
    font: HFONT,
    dpi: u32,
    metrics: MenuDpiMetrics,
    palette: TrayThemePalette,
    registry: NativeMenuHintRegistry,
    root_menu: HMENU,
}

#[derive(Clone, Copy)]
struct MenuDpiMetrics {
    item_height: u32,
    separator_height: u32,
    item_width: u32,
    check_gutter: i32,
    submenu_gutter: i32,
    item_inset: i32,
}

struct MenuDpiResources {
    dpi: u32,
    metrics: MenuDpiMetrics,
    font: HFONT,
}

#[derive(Clone, Default)]
struct NativeMenuHintRegistry {
    command_items: HashMap<u32, TrayHelpKey>,
    command_locations: HashMap<u32, NativeMenuItemLocation>,
    submenus: HashMap<isize, TrayHelpKey>,
    submenu_locations: Vec<(HMENU, u32, TrayHelpKey)>,
}

/// A small native tooltip controller owned exclusively by the tray UI thread.
/// Native menu items have no child HWNDs, so WM_MENUSELECT remains the
/// authority for selection. The controller uses a documented Common Controls
/// tracking tooltip and never owns Agent, policy, runner, or probe state.
pub struct MenuHintTooltip {
    host: HintHostWindow,
    tray_owner: HWND,
    pending: Option<PendingHint>,
    registry: NativeMenuHintRegistry,
    locale: crate::EffectiveLocale,
    hints_enabled: bool,
    palette: TrayThemePalette,
    explicit_theme: bool,
    selected_hint_key: Option<TrayHelpKey>,
    active_hint_key: Option<TrayHelpKey>,
    generation: u64,
    queued_dismissal: Option<HintDismissal>,
    due_at: Option<Instant>,
    visible_since: Option<Instant>,
    dpi_refresh_pending: bool,
    evidence: NativeHintEvidence,
    evidence_dirty: bool,
}

struct PendingHint {
    text: String,
    palette: TrayThemePalette,
    anchor: Option<RECT>,
}

#[derive(Clone, Copy)]
struct NativeMenuItemLocation {
    menu: HMENU,
    position: u32,
}

#[derive(Clone)]
struct ResolvedMenuSelection {
    key: TrayHelpKey,
    location: Option<NativeMenuItemLocation>,
}

/// The hidden native host separates RunnerMesh's tooltip lifecycle from the
/// tray-icon/muda owner window. It lives on the same UI thread and owns no
/// Agent behaviour: its only responsibilities are the timer, Common Controls
/// tooltip notifications, and native hint presentation.
struct HintHostWindow {
    window: HWND,
    tooltip: Option<NativeTooltipBackend>,
}

struct TooltipRegistration {
    common_controls_version: u32,
    toolinfo_cb_size: u32,
    tool_count_before: isize,
    tool_count_after: isize,
    add_return: bool,
    gettoolinfo_return: bool,
}

#[derive(Clone, Copy)]
struct TooltipAbiLayout {
    size: usize,
    cb_size: usize,
    flags: usize,
    hwnd: usize,
    uid: usize,
    rect: usize,
    hinst: usize,
    text: usize,
    lparam: usize,
    reserved: usize,
    v1_size: u32,
    v2_size: u32,
    v3_size: u32,
}

#[derive(Default)]
struct ComctlRuntimeProbe {
    loaded: bool,
    dll_get_version: bool,
    major: u32,
    minor: u32,
    path_class: &'static str,
    app_themed: bool,
    theme_active: bool,
}

struct TooltipMatrix {
    v3: TooltipRegistration,
    v2: TooltipRegistration,
    v1: TooltipRegistration,
}

const HINT_VISIBLE_MINIMUM: Duration = Duration::from_millis(250);
const TOOLTIP_MAX_WIDTH_LOGICAL: i32 = 320;
const TOOLTIP_GAP_LOGICAL: i32 = 10;

struct NativeTooltipBackend {
    window: HWND,
    // The common control may retain this pointer after TTM_ADDTOOLW.
    _registration_text: Vec<u16>,
    active_text: Vec<u16>,
    tool: TTTOOLINFOW,
    registered: bool,
    custom_palette: Option<TrayThemePalette>,
    custom_draw_count: u64,
}

fn tooltip_abi_layout() -> TooltipAbiLayout {
    TooltipAbiLayout {
        size: size_of::<TTTOOLINFOW>(),
        cb_size: offset_of!(TTTOOLINFOW, cbSize),
        flags: offset_of!(TTTOOLINFOW, uFlags),
        hwnd: offset_of!(TTTOOLINFOW, hwnd),
        uid: offset_of!(TTTOOLINFOW, uId),
        rect: offset_of!(TTTOOLINFOW, rect),
        hinst: offset_of!(TTTOOLINFOW, hinst),
        text: offset_of!(TTTOOLINFOW, lpszText),
        lparam: offset_of!(TTTOOLINFOW, lParam),
        reserved: offset_of!(TTTOOLINFOW, lpReserved),
        v1_size: (offset_of!(TTTOOLINFOW, lpszText) + size_of::<*mut u16>()) as u32,
        v2_size: (offset_of!(TTTOOLINFOW, lParam) + size_of::<LPARAM>()) as u32,
        v3_size: (offset_of!(TTTOOLINFOW, lpReserved) + size_of::<*mut core::ffi::c_void>()) as u32,
    }
}

unsafe fn initialize_common_controls() -> bool {
    let common_controls = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_WIN95_CLASSES,
    };
    InitCommonControlsEx(&common_controls) != 0
}

unsafe fn inspect_comctl_runtime() -> ComctlRuntimeProbe {
    let module_name = wide_null("comctl32.dll");
    let module = GetModuleHandleW(module_name.as_ptr());
    let mut probe = ComctlRuntimeProbe {
        loaded: !module.is_null(),
        app_themed: IsAppThemed() != 0,
        theme_active: IsThemeActive() != 0,
        ..Default::default()
    };
    if module.is_null() {
        return probe;
    }

    let mut path_buffer = vec![0_u16; 32_768];
    let path_length =
        GetModuleFileNameW(module, path_buffer.as_mut_ptr(), path_buffer.len() as u32);
    if path_length > 0 && path_length < path_buffer.len() as u32 {
        let path =
            String::from_utf16_lossy(&path_buffer[..path_length as usize]).to_ascii_lowercase();
        probe.path_class = if path.contains("\\winsxs\\") {
            "winsxs"
        } else if path.contains("\\system32\\") {
            "system"
        } else {
            "other-sanitized"
        };
    }

    let Some(raw_proc) = GetProcAddress(module, c"DllGetVersion".as_ptr().cast()) else {
        return probe;
    };
    let get_version: DLLGETVERSIONPROC = Some(std::mem::transmute::<
        unsafe extern "system" fn() -> isize,
        unsafe extern "system" fn(*mut DLLVERSIONINFO) -> i32,
    >(raw_proc));
    let mut version = DLLVERSIONINFO {
        cbSize: size_of::<DLLVERSIONINFO>() as u32,
        ..Default::default()
    };
    if let Some(get_version) = get_version {
        if get_version(&mut version) >= 0 {
            probe.dll_get_version = true;
            probe.major = version.dwMajorVersion;
            probe.minor = version.dwMinorVersion;
        }
    }
    probe
}

unsafe fn run_tooltip_matrix(host: HWND) -> Result<TooltipMatrix, String> {
    let layout = tooltip_abi_layout();
    let mut tooltip = NativeTooltipBackend::new(host)?;
    let v3 = tooltip.add_tool_with_cb_size(layout.v3_size, true);
    let v2 = tooltip.add_tool_with_cb_size(layout.v2_size, true);
    let v1 = tooltip.add_tool_with_cb_size(layout.v1_size, true);
    tooltip.dispose();
    Ok(TooltipMatrix { v3, v2, v1 })
}

impl NativeTooltipBackend {
    unsafe fn new(host: HWND) -> Result<Self, String> {
        if !initialize_common_controls() {
            return Err("could not initialize Windows Common Controls for tray hints".to_owned());
        }
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            TOOLTIPS_CLASSW,
            std::ptr::null(),
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
            0,
            0,
            0,
            0,
            host,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()) as _,
            std::ptr::null(),
        );
        if window.is_null() {
            return Err("could not create the Windows Common Controls tray tooltip".to_owned());
        }
        SetWindowPos(
            window,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        let mut registration_text = wide_null("RunnerMesh");
        let active_text = vec![0_u16];
        let flags = TTF_TRACK | TTF_ABSOLUTE;
        let tool_id = HINT_VIRTUAL_TOOL_ID;
        let rect = RECT {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        };
        let tool = TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            uFlags: flags,
            hwnd: host,
            hinst: GetModuleHandleW(std::ptr::null()) as _,
            uId: tool_id,
            rect,
            lpszText: registration_text.as_mut_ptr(),
            ..Default::default()
        };
        let mut backend = Self {
            window,
            _registration_text: registration_text,
            active_text,
            tool,
            registered: false,
            custom_palette: None,
            custom_draw_count: 0,
        };
        backend.set_max_width_for_dpi(GetDpiForWindow(host).max(96));
        Ok(backend)
    }

    unsafe fn show(
        &mut self,
        text: &str,
        palette: TrayThemePalette,
        explicit_theme: bool,
        menu_item_anchor: Option<RECT>,
        tray_owner: HWND,
    ) -> Result<TooltipWindowState, String> {
        if !self.registered {
            return Err("the Windows Common Controls tray tooltip is not registered".to_owned());
        }
        if text.is_empty() {
            return Err("a native tray tooltip cannot display empty text".to_owned());
        }
        self.active_text = wide_null(text);
        self.tool.lpszText = self.active_text.as_mut_ptr();
        SendMessageW(
            self.window,
            TTM_UPDATETIPTEXTW,
            0,
            (&self.tool as *const TTTOOLINFOW) as LPARAM,
        );
        self.custom_palette = explicit_theme.then_some(palette);
        self.set_max_width_for_dpi(GetDpiForWindow(tray_owner).max(96));
        let bubble_size = self.bubble_size().ok_or_else(|| {
            "could not determine the native Windows tooltip bubble size".to_owned()
        })?;
        let mut context = tooltip_placement_context(
            menu_item_anchor,
            GetDpiForWindow(tray_owner).max(96),
            bubble_size,
        )?;
        let mut placement = choose_tooltip_placement(
            context.anchor,
            context.bubble_size,
            context.work_area,
            context.gap,
        );
        self.position_and_activate(placement.rect.left, placement.rect.top);
        let mut state = self.state(&context, placement);

        // Common Controls owns the final border metrics. If they differ from
        // TTM_GETBUBBLESIZE, re-fit using the actual native window dimensions
        // while still using TTM_TRACKPOSITION as the positioning authority.
        let tooltip_dpi = GetDpiForWindow(self.window).max(96);
        self.set_max_width_for_dpi(tooltip_dpi);
        if let Some(actual_size) = self
            .bubble_size()
            .or_else(|| tooltip_size_from_rect(state.rect))
        {
            context = tooltip_placement_context(menu_item_anchor, tooltip_dpi, actual_size)?;
            let refined = choose_tooltip_placement(
                context.anchor,
                actual_size,
                context.work_area,
                context.gap,
            );
            if refined.rect.left != placement.rect.left || refined.rect.top != placement.rect.top {
                placement = refined;
                self.position_and_activate(placement.rect.left, placement.rect.top);
                state = self.state(&context, placement);
            }
        }
        Ok(state)
    }

    unsafe fn hide(&mut self) {
        if !self.registered {
            return;
        }
        SendMessageW(
            self.window,
            TTM_TRACKACTIVATE,
            0,
            (&self.tool as *const TTTOOLINFOW) as LPARAM,
        );
    }

    unsafe fn dispose(&mut self) {
        self.hide();
        if self.registered {
            SendMessageW(
                self.window,
                TTM_DELTOOLW,
                0,
                (&self.tool as *const TTTOOLINFOW) as LPARAM,
            );
            self.registered = false;
        }
        if !self.window.is_null() {
            DestroyWindow(self.window);
            self.window = std::ptr::null_mut();
        }
    }

    unsafe fn position_and_activate(&self, x: i32, y: i32) {
        SendMessageW(self.window, TTM_TRACKPOSITION, 0, pack_screen_point(x, y));
        SendMessageW(
            self.window,
            TTM_TRACKACTIVATE,
            1,
            (&self.tool as *const TTTOOLINFOW) as LPARAM,
        );
    }

    unsafe fn bubble_size(&self) -> Option<TooltipSize> {
        let packed = SendMessageW(
            self.window,
            TTM_GETBUBBLESIZE,
            0,
            (&self.tool as *const TTTOOLINFOW) as LPARAM,
        ) as u32;
        let size = TooltipSize {
            width: (packed & 0xffff) as i32,
            height: (packed >> 16) as i32,
        };
        (size.width > 0 && size.height > 0).then_some(size)
    }

    unsafe fn set_max_width_for_dpi(&mut self, dpi: u32) {
        let max_width = scale_logical_px(TOOLTIP_MAX_WIDTH_LOGICAL, dpi);
        SendMessageW(self.window, TTM_SETMAXTIPWIDTH, 0, max_width as LPARAM);
    }

    unsafe fn state(
        &self,
        context: &TooltipPlacementContext,
        placement: AdaptiveTooltipPlacement,
    ) -> TooltipWindowState {
        let mut rect = RECT::default();
        let rect_read = GetWindowRect(self.window, &mut rect) != 0;
        let exstyle = GetWindowLongPtrW(self.window, GWL_EXSTYLE) as u32;
        TooltipWindowState {
            valid: IsWindow(self.window) != 0,
            visible: IsWindowVisible(self.window) != 0,
            topmost: exstyle & WS_EX_TOPMOST != 0,
            nonactivating: exstyle & WS_EX_NOACTIVATE != 0,
            on_monitor: rect_read && rect_inside(rect, context.work_area),
            overlaps_active_menu: rect_read && rects_intersect(rect, context.anchor),
            bubble_size_valid: context.bubble_size.width > 0 && context.bubble_size.height > 0,
            monitor_work_area_valid: valid_rect(context.work_area),
            placement_direction: placement.direction,
            placement_clamped: placement.clamped,
            used_menu_item_anchor: context.used_menu_item_anchor,
            tooltip_dpi: GetDpiForWindow(self.window).max(96),
            rect,
        }
    }

    unsafe fn registered_and_text_known(&self) -> bool {
        if !self.registered {
            return false;
        }
        let mut tool = TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            uFlags: self.tool.uFlags,
            hwnd: self.tool.hwnd,
            uId: self.tool.uId,
            ..Default::default()
        };
        SendMessageW(
            self.window,
            TTM_GETTOOLINFOW,
            0,
            (&mut tool as *mut TTTOOLINFOW) as LPARAM,
        ) != 0
            && !tool.lpszText.is_null()
            && self.active_text.len() > 1
    }

    unsafe fn add_tool_with_cb_size(
        &mut self,
        cb_size: u32,
        remove_after: bool,
    ) -> TooltipRegistration {
        let common_controls_version = SendMessageW(self.window, CCM_GETVERSION, 0, 0) as u32;
        let tool_count_before = SendMessageW(self.window, TTM_GETTOOLCOUNT, 0, 0);
        self.tool.cbSize = cb_size;
        let add_return = SendMessageW(
            self.window,
            TTM_ADDTOOLW,
            0,
            (&self.tool as *const TTTOOLINFOW) as LPARAM,
        ) != 0;
        let mut tool = TTTOOLINFOW {
            cbSize: cb_size,
            uFlags: self.tool.uFlags,
            hwnd: self.tool.hwnd,
            uId: self.tool.uId,
            ..Default::default()
        };
        let gettoolinfo_return = SendMessageW(
            self.window,
            TTM_GETTOOLINFOW,
            0,
            (&mut tool as *mut TTTOOLINFOW) as LPARAM,
        ) != 0;
        let tool_count_after = SendMessageW(self.window, TTM_GETTOOLCOUNT, 0, 0);
        let accepted = add_return && gettoolinfo_return;
        if accepted && remove_after {
            SendMessageW(
                self.window,
                TTM_DELTOOLW,
                0,
                (&self.tool as *const TTTOOLINFOW) as LPARAM,
            );
        }
        self.registered = accepted && !remove_after;
        TooltipRegistration {
            common_controls_version,
            toolinfo_cb_size: cb_size,
            tool_count_before,
            tool_count_after,
            add_return,
            gettoolinfo_return,
        }
    }

    unsafe fn handle_custom_draw(&mut self, lparam: LPARAM) -> Option<LRESULT> {
        let draw = (lparam as *mut NMTTCUSTOMDRAW).as_mut()?;
        if draw.nmcd.hdr.hwndFrom != self.window || draw.nmcd.hdr.code != NM_CUSTOMDRAW {
            return None;
        }
        self.custom_draw_count += 1;
        let Some(palette) = self.custom_palette else {
            return Some(CDRF_DODEFAULT as LRESULT);
        };
        if draw.nmcd.dwDrawStage != CDDS_PREPAINT {
            return Some(CDRF_DODEFAULT as LRESULT);
        }
        let mut client = RECT::default();
        GetClientRect(self.window, &mut client);
        let brush = CreateSolidBrush(palette.surface);
        if !brush.is_null() {
            FillRect(draw.nmcd.hdc, &client, brush);
            DeleteObject(brush as _);
        }
        SetTextColor(draw.nmcd.hdc, palette.text);
        SetBkMode(draw.nmcd.hdc, TRANSPARENT as i32);
        let mut margin = RECT::default();
        SendMessageW(
            self.window,
            TTM_GETMARGIN,
            0,
            (&mut margin as *mut RECT) as LPARAM,
        );
        let mut text = client;
        text.left += margin.left;
        text.top += margin.top;
        text.right -= margin.right;
        text.bottom -= margin.bottom;
        DrawTextW(
            draw.nmcd.hdc,
            self.active_text.as_ptr(),
            self.active_text.len().saturating_sub(1) as i32,
            &mut text,
            DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
        );
        Some(CDRF_SKIPDEFAULT as LRESULT)
    }
}

impl HintHostWindow {
    const fn dormant() -> Self {
        Self {
            window: std::ptr::null_mut(),
            tooltip: None,
        }
    }

    unsafe fn create(controller: *mut MenuHintTooltip) -> Result<Self, String> {
        ensure_hint_host_class()?;
        let class_name = wide_null(HINT_HOST_CLASS_NAME);
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()) as _,
            controller.cast(),
        );
        if window.is_null() {
            return Err("could not create the hidden Windows tray hint host".to_owned());
        }
        Ok(Self {
            window,
            tooltip: None,
        })
    }

    unsafe fn post(&self, message: u32, generation: u64) -> bool {
        !self.window.is_null() && PostMessageW(self.window, message, generation as usize, 0) != 0
    }

    unsafe fn dispose(&mut self) {
        if let Some(tooltip) = self.tooltip.as_mut() {
            tooltip.dispose();
        }
        self.tooltip = None;
        if !self.window.is_null() {
            SetWindowLongPtrW(self.window, GWLP_USERDATA, 0);
            DestroyWindow(self.window);
            self.window = std::ptr::null_mut();
        }
    }
}

fn ensure_hint_host_class() -> Result<(), String> {
    HINT_HOST_CLASS
        .get_or_init(|| unsafe {
            let name = wide_null(HINT_HOST_CLASS_NAME);
            let class = WNDCLASSW {
                lpfnWndProc: Some(hint_host_proc),
                hInstance: GetModuleHandleW(std::ptr::null()) as _,
                lpszClassName: name.as_ptr(),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                Err("could not register the hidden Windows tray hint-host class".to_owned())
            } else {
                Ok(())
            }
        })
        .clone()
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone, Copy)]
struct TooltipWindowState {
    valid: bool,
    visible: bool,
    topmost: bool,
    nonactivating: bool,
    on_monitor: bool,
    overlaps_active_menu: bool,
    bubble_size_valid: bool,
    monitor_work_area_valid: bool,
    placement_direction: TooltipPlacementDirection,
    placement_clamped: bool,
    used_menu_item_anchor: bool,
    tooltip_dpi: u32,
    rect: RECT,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TooltipPlacementDirection {
    Right,
    Left,
    Below,
    Above,
}

impl TooltipPlacementDirection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Below => "below",
            Self::Above => "above",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TooltipSize {
    width: i32,
    height: i32,
}

#[derive(Clone, Copy)]
struct TooltipPlacementContext {
    anchor: RECT,
    work_area: RECT,
    bubble_size: TooltipSize,
    gap: i32,
    used_menu_item_anchor: bool,
}

#[derive(Clone, Copy)]
struct AdaptiveTooltipPlacement {
    direction: TooltipPlacementDirection,
    rect: RECT,
    clamped: bool,
}

unsafe fn tooltip_placement_context(
    menu_item_anchor: Option<RECT>,
    dpi: u32,
    bubble_size: TooltipSize,
) -> Result<TooltipPlacementContext, String> {
    let mut cursor = POINT::default();
    let used_menu_item_anchor = menu_item_anchor.is_some();
    let anchor = match menu_item_anchor {
        Some(anchor) => anchor,
        None => {
            if GetCursorPos(&mut cursor) == 0 {
                return Err(
                    "could not read the cursor position for the native tray tooltip".to_owned(),
                );
            }
            RECT {
                left: cursor.x,
                top: cursor.y,
                right: cursor.x.saturating_add(1),
                bottom: cursor.y.saturating_add(1),
            }
        }
    };
    let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
    if monitor.is_null() {
        return Err("could not identify a monitor for the native tray tooltip".to_owned());
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(monitor, &mut info) == 0 {
        return Err("could not read monitor bounds for the native tray tooltip".to_owned());
    }
    let gap = scale_logical_px(TOOLTIP_GAP_LOGICAL, dpi);
    Ok(TooltipPlacementContext {
        anchor,
        work_area: info.rcWork,
        bubble_size,
        gap,
        used_menu_item_anchor,
    })
}

fn choose_tooltip_placement(
    anchor: RECT,
    tooltip: TooltipSize,
    work_area: RECT,
    gap: i32,
) -> AdaptiveTooltipPlacement {
    let candidates = [
        (
            TooltipPlacementDirection::Right,
            rect_at(anchor.right.saturating_add(gap), anchor.top, tooltip),
        ),
        (
            TooltipPlacementDirection::Left,
            rect_at(
                anchor
                    .left
                    .saturating_sub(tooltip.width.saturating_add(gap)),
                anchor.top,
                tooltip,
            ),
        ),
        (
            TooltipPlacementDirection::Below,
            rect_at(anchor.left, anchor.bottom.saturating_add(gap), tooltip),
        ),
        (
            TooltipPlacementDirection::Above,
            rect_at(
                anchor.left,
                anchor
                    .top
                    .saturating_sub(tooltip.height.saturating_add(gap)),
                tooltip,
            ),
        ),
    ];

    let (_, (direction, candidate)) = candidates
        .into_iter()
        .enumerate()
        .min_by_key(|(priority, (_, candidate))| {
            (
                !rect_inside(*candidate, work_area),
                rects_intersect(*candidate, anchor),
                rect_overflow_area(*candidate, work_area),
                *priority,
            )
        })
        .expect("a native tooltip always has four placement candidates");
    let rect = clamp_rect_to_work_area(candidate, work_area);
    AdaptiveTooltipPlacement {
        direction,
        clamped: !rects_equal(rect, candidate),
        rect,
    }
}

fn rect_at(left: i32, top: i32, size: TooltipSize) -> RECT {
    RECT {
        left,
        top,
        right: left.saturating_add(size.width.max(1)),
        bottom: top.saturating_add(size.height.max(1)),
    }
}

fn tooltip_size_from_rect(rect: RECT) -> Option<TooltipSize> {
    let size = TooltipSize {
        width: rect.right.saturating_sub(rect.left),
        height: rect.bottom.saturating_sub(rect.top),
    };
    (size.width > 0 && size.height > 0).then_some(size)
}

const fn pack_screen_point(x: i32, y: i32) -> LPARAM {
    ((x as u32 & 0xffff) | ((y as u32 & 0xffff) << 16)) as LPARAM
}

fn clamp_to_work_area(value: i32, lower: i32, upper: i32, size: i32) -> i32 {
    let maximum = (upper - size).max(lower);
    value.clamp(lower, maximum)
}

fn clamp_rect_to_work_area(rect: RECT, work_area: RECT) -> RECT {
    let size = tooltip_size_from_rect(rect).unwrap_or(TooltipSize {
        width: 1,
        height: 1,
    });
    rect_at(
        clamp_to_work_area(rect.left, work_area.left, work_area.right, size.width),
        clamp_to_work_area(rect.top, work_area.top, work_area.bottom, size.height),
        size,
    )
}

fn valid_rect(rect: RECT) -> bool {
    rect.right > rect.left && rect.bottom > rect.top
}

fn rect_inside(rect: RECT, container: RECT) -> bool {
    rect.left >= container.left
        && rect.top >= container.top
        && rect.right <= container.right
        && rect.bottom <= container.bottom
}

fn rects_equal(left: RECT, right: RECT) -> bool {
    left.left == right.left
        && left.top == right.top
        && left.right == right.right
        && left.bottom == right.bottom
}

fn rect_overflow_area(rect: RECT, bounds: RECT) -> i64 {
    let width = i64::from(rect.right.saturating_sub(rect.left).max(0));
    let height = i64::from(rect.bottom.saturating_sub(rect.top).max(0));
    let total = width * height;
    let intersection_left = rect.left.max(bounds.left);
    let intersection_top = rect.top.max(bounds.top);
    let intersection_right = rect.right.min(bounds.right);
    let intersection_bottom = rect.bottom.min(bounds.bottom);
    let intersected_width = i64::from(intersection_right.saturating_sub(intersection_left).max(0));
    let intersected_height = i64::from(intersection_bottom.saturating_sub(intersection_top).max(0));
    total.saturating_sub(intersected_width * intersected_height)
}

fn rects_intersect(left: RECT, right: RECT) -> bool {
    left.left < right.right
        && left.right > right.left
        && left.top < right.bottom
        && left.bottom > right.top
}

impl MenuHintTooltip {
    /// # Safety
    ///
    /// `owner` must be the live tray-icon window on the UI event-loop thread.
    pub unsafe fn new(
        tray_owner: HWND,
        mut stage: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<Box<Self>, String> {
        let mut controller = Box::new(Self {
            host: HintHostWindow::dormant(),
            tray_owner,
            pending: None,
            registry: NativeMenuHintRegistry::default(),
            locale: crate::EffectiveLocale::EnUs,
            hints_enabled: true,
            palette: TrayThemePalette::for_effective(EffectiveTheme::Light),
            explicit_theme: false,
            selected_hint_key: None,
            active_hint_key: None,
            generation: 0,
            queued_dismissal: None,
            due_at: None,
            visible_since: None,
            dpi_refresh_pending: false,
            evidence: NativeHintEvidence {
                hint_backend_created: true,
                tooltip_common_controls_v6_manifest: true,
                tooltip_font_system_native: true,
                tooltip_max_width_configured: true,
                thread_dpi_awareness: GetAwarenessFromDpiAwarenessContext(
                    GetThreadDpiAwarenessContext(),
                ),
                per_monitor_v2_thread: is_per_monitor_v2_context(GetThreadDpiAwarenessContext()),
                per_monitor_v2_tray_owner: is_per_monitor_v2_context(GetWindowDpiAwarenessContext(
                    tray_owner,
                )),
                tray_owner_dpi: GetDpiForWindow(tray_owner).max(96),
                tray_icon_size_px: small_icon_size_for_window(tray_owner),
                no_bitmap_scale_fallback: true,
                ..Default::default()
            },
            evidence_dirty: true,
        });
        let pointer = controller.as_mut() as *mut MenuHintTooltip;
        controller.host = HintHostWindow::create(pointer)?;
        controller.evidence.hint_host_created = true;
        controller.evidence.hint_host_dpi = GetDpiForWindow(controller.host.window).max(96);
        controller.evidence.per_monitor_v2_hint_host =
            is_per_monitor_v2_context(GetWindowDpiAwarenessContext(controller.host.window));
        if !controller.host.post(HINT_HOST_INIT_NATIVE_TOOLTIP, 0) {
            controller.host.dispose();
            return Err(
                "could not queue native tooltip initialization on the hint host".to_owned(),
            );
        }
        stage("native-hint-host-created")?;
        Ok(controller)
    }

    pub fn update_menu_presentation(
        &mut self,
        themed: &ThemedMenu,
        render: &TrayRender,
        explicit_theme: bool,
    ) {
        self.registry = themed.registry.clone();
        self.locale = render.locale;
        self.hints_enabled = render.menu_hints_enabled;
        self.palette = themed.palette;
        self.explicit_theme = explicit_theme;
        if let Some(tooltip) = self.host.tooltip.as_mut() {
            tooltip.custom_palette = explicit_theme.then_some(themed.palette);
        }
        unsafe {
            self.evidence.tray_owner_dpi = GetDpiForWindow(self.tray_owner).max(96);
            self.evidence.tray_icon_size_px = small_icon_size_for_window(self.tray_owner);
        }
        self.evidence.menu_font_dpi = themed.dpi;
        self.evidence.menu_metrics_dpi = themed.dpi;
        self.evidence.no_bitmap_scale_fallback = !themed.font.is_null();
        self.evidence_dirty = true;
    }

    /// Returns and clears an owner-window DPI transition observed by the tray
    /// subclass. Rebuilding the presentation remains the runtime's UI-thread
    /// responsibility; this component never changes Agent state.
    pub fn take_dpi_refresh_request(&mut self) -> bool {
        let pending = self.dpi_refresh_pending;
        self.dpi_refresh_pending = false;
        pending
    }

    unsafe fn note_dpi_changed(&mut self) {
        self.dpi_refresh_pending = true;
        self.evidence.dpi_change_count += 1;
        self.evidence.tray_owner_dpi = GetDpiForWindow(self.tray_owner).max(96);
        self.evidence.tray_icon_size_px = small_icon_size_for_window(self.tray_owner);
        self.evidence.per_monitor_v2_tray_owner =
            is_per_monitor_v2_context(GetWindowDpiAwarenessContext(self.tray_owner));
        self.evidence_dirty = true;
    }

    pub fn take_evidence(&mut self) -> Option<NativeHintEvidence> {
        if self.evidence_dirty {
            self.evidence_dirty = false;
            Some(self.evidence.clone())
        } else {
            None
        }
    }

    unsafe fn select(&mut self, selection: Option<ResolvedMenuSelection>) {
        self.evidence.menu_select_count += 1;
        self.evidence_dirty = true;
        self.generation = self.generation.wrapping_add(1);
        self.selected_hint_key = selection.as_ref().map(|selection| selection.key.clone());
        self.queued_dismissal = Some(HintDismissal::SelectionChanged);
        self.pending = if !self.hints_enabled {
            None
        } else if let Some(selection) = selection {
            let key = selection.key;
            let anchor = selection
                .location
                .and_then(|location| self.menu_item_rect(location));
            self.evidence.active_menu_item_rect_valid |= anchor.is_some();
            self.evidence.hint_key_resolution_count += 1;
            mark_hint_key(&mut self.evidence, &key);
            localized_menu_hint(&key, self.locale).map(|text| PendingHint {
                text: text.to_owned(),
                palette: self.palette,
                anchor,
            })
        } else {
            None
        };
        self.host.post(HINT_HOST_SELECTION_CHANGED, self.generation);
    }

    unsafe fn menu_item_rect(&self, location: NativeMenuItemLocation) -> Option<RECT> {
        let mut rect = RECT::default();
        if GetMenuItemRect(self.tray_owner, location.menu, location.position, &mut rect) != 0 {
            return Some(rect);
        }
        // For a displayed popup, NULL asks User32 to locate the menu window.
        // This is the documented fallback when the hidden tray owner is not
        // itself the popup's containing window.
        (GetMenuItemRect(
            std::ptr::null_mut(),
            location.menu,
            location.position,
            &mut rect,
        ) != 0)
            .then_some(rect)
    }

    unsafe fn show_pending_on_host(&mut self) {
        KillTimer(self.host.window, HINT_TIMER_ID);
        let Some(pending) = self.pending.take() else {
            return;
        };
        let Some(tooltip) = self.host.tooltip.as_mut() else {
            return;
        };
        if !tooltip.registered {
            return;
        }
        self.active_hint_key = self.selected_hint_key.clone();
        self.due_at = None;
        self.visible_since = Some(Instant::now());
        self.evidence.tooltip_activation_request_count += 1;
        self.evidence.hint_text_nonempty = !pending.text.is_empty();
        if let Ok(state) = tooltip.show(
            &pending.text,
            pending.palette,
            self.explicit_theme,
            pending.anchor,
            self.tray_owner,
        ) {
            self.evidence.hint_window_valid = state.valid;
            self.evidence.hint_window_visible = state.visible;
            self.evidence.hint_window_topmost = state.topmost;
            self.evidence.tooltip_nonactivating = state.nonactivating;
            self.evidence.hint_window_on_monitor = state.on_monitor;
            self.evidence.tooltip_overlaps_active_menu = state.overlaps_active_menu;
            self.evidence.tooltip_bubble_size_valid = state.bubble_size_valid;
            self.evidence.monitor_work_area_valid = state.monitor_work_area_valid;
            self.evidence.tooltip_rect_inside_work_area = state.on_monitor;
            self.evidence.tooltip_placement_direction =
                state.placement_direction.as_str().to_owned();
            self.evidence.tooltip_placement_clamped = state.placement_clamped;
            self.evidence.tooltip_used_menu_item_anchor = state.used_menu_item_anchor;
            self.evidence.tooltip_dpi = state.tooltip_dpi;
            self.evidence.per_monitor_v2_tooltip =
                is_per_monitor_v2_context(GetWindowDpiAwarenessContext(tooltip.window));
            self.evidence.tooltip_text_readback = tooltip.registered_and_text_known();
            self.evidence.hint_paint_count = tooltip.custom_draw_count;
        }
        self.evidence_dirty = true;
    }

    unsafe fn deactivate_on_host(&mut self, dismissal: HintDismissal) {
        KillTimer(self.host.window, HINT_TIMER_ID);
        self.due_at = None;
        if let Some(tooltip) = self.host.tooltip.as_mut() {
            tooltip.hide();
        }
        self.evidence.hint_visible_lifetime |= self
            .visible_since
            .map(|due| due.elapsed() >= HINT_VISIBLE_MINIMUM)
            .unwrap_or(false);
        self.visible_since = None;
        match dismissal {
            HintDismissal::MenuClosed => self.evidence.hint_hide_on_menu_close = true,
            HintDismissal::Disabled => self.evidence.hint_hide_when_disabled = true,
            HintDismissal::SelectionChanged | HintDismissal::Rebuild | HintDismissal::Shutdown => {}
        }
        if self.active_hint_key.take().is_some() {
            self.evidence.tooltip_deactivation_count += 1;
        }
        self.evidence_dirty = true;
    }

    unsafe fn queue_dismiss(&mut self, dismissal: HintDismissal) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        self.queued_dismissal = Some(dismissal);
        self.host.post(HINT_HOST_DISMISS, self.generation);
    }

    unsafe fn handle_host_message(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        match message {
            HINT_HOST_INIT_NATIVE_TOOLTIP => {
                self.evidence.hint_host_init_message_received = true;
                let abi = tooltip_abi_layout();
                self.evidence.tooltip_layout_size = abi.size;
                self.evidence.tooltip_offset_cb_size = abi.cb_size;
                self.evidence.tooltip_offset_flags = abi.flags;
                self.evidence.tooltip_offset_hwnd = abi.hwnd;
                self.evidence.tooltip_offset_uid = abi.uid;
                self.evidence.tooltip_offset_rect = abi.rect;
                self.evidence.tooltip_offset_hinst = abi.hinst;
                self.evidence.tooltip_offset_text = abi.text;
                self.evidence.tooltip_offset_lparam = abi.lparam;
                self.evidence.tooltip_offset_reserved = abi.reserved;
                self.evidence.tooltip_v1_size = abi.v1_size;
                self.evidence.tooltip_v2_size = abi.v2_size;
                self.evidence.tooltip_v3_size = abi.v3_size;

                if initialize_common_controls() {
                    let before = inspect_comctl_runtime();
                    self.evidence.comctl_before_tooltip_loaded = before.loaded;
                    let matrix = run_tooltip_matrix(hwnd);
                    let after = inspect_comctl_runtime();
                    self.evidence.comctl_after_tooltip_loaded = after.loaded;
                    self.evidence.comctl_dll_major = after.major;
                    self.evidence.comctl_dll_minor = after.minor;
                    self.evidence.comctl_module_path_class = after.path_class.to_owned();
                    self.evidence.comctl6_active = after.dll_get_version && after.major >= 6;
                    self.evidence.app_themed = after.app_themed;
                    self.evidence.theme_active = after.theme_active;

                    if let Ok(matrix) = matrix {
                        self.evidence.tooltip_matrix_v3_add = matrix.v3.add_return;
                        self.evidence.tooltip_matrix_v3_get = matrix.v3.gettoolinfo_return;
                        self.evidence.tooltip_matrix_v2_add = matrix.v2.add_return;
                        self.evidence.tooltip_matrix_v2_get = matrix.v2.gettoolinfo_return;
                        self.evidence.tooltip_matrix_v1_add = matrix.v1.add_return;
                        self.evidence.tooltip_matrix_v1_get = matrix.v1.gettoolinfo_return;
                    }
                }

                // The production presentation path uses the documented full
                // structure only after the isolated ABI matrix accepts it.
                if self.evidence.tooltip_matrix_v3_add && self.evidence.tooltip_matrix_v3_get {
                    match NativeTooltipBackend::new(hwnd) {
                        Ok(mut tooltip) => {
                            tooltip.custom_palette = self.explicit_theme.then_some(self.palette);
                            self.evidence.tooltip_window_created = true;
                            self.evidence.tooltip_window_valid_before_registration =
                                IsWindow(tooltip.window) != 0;
                            self.evidence.tooltip_dpi = GetDpiForWindow(tooltip.window).max(96);
                            self.evidence.per_monitor_v2_tooltip = is_per_monitor_v2_context(
                                GetWindowDpiAwarenessContext(tooltip.window),
                            );
                            self.evidence.tooltip_tool_host_is_hint_host =
                                tooltip.tool.hwnd == hwnd;
                            self.evidence.tooltip_virtual_flags = tooltip.tool.uFlags;
                            let registration = tooltip.add_tool_with_cb_size(abi.v3_size, false);
                            self.evidence.tooltip_common_controls_version =
                                registration.common_controls_version;
                            self.evidence.tooltip_toolinfo_cb_size = registration.toolinfo_cb_size;
                            self.evidence.tooltip_tool_count_before =
                                registration.tool_count_before;
                            self.evidence.tooltip_tool_count_after = registration.tool_count_after;
                            self.evidence.tooltip_add_return = registration.add_return;
                            self.evidence.tooltip_gettoolinfo_return =
                                registration.gettoolinfo_return;
                            self.evidence.tooltip_identity_virtual_numeric = true;
                            self.evidence.tooltip_virtual_add_return = registration.add_return;
                            self.evidence.tooltip_virtual_gettoolinfo_return =
                                registration.gettoolinfo_return;
                            self.evidence.tooltip_tool_registered = tooltip.registered;
                            self.host.tooltip = Some(tooltip);
                        }
                        Err(_) => self.evidence.tooltip_tool_registered = false,
                    }
                }
                self.evidence_dirty = true;
                Some(0)
            }
            HINT_HOST_SELECTION_CHANGED if wparam as u64 == self.generation => {
                self.deactivate_on_host(HintDismissal::SelectionChanged);
                if self.pending.is_some()
                    && self.hints_enabled
                    && self
                        .host
                        .tooltip
                        .as_ref()
                        .is_some_and(|tooltip| tooltip.registered)
                    && SetTimer(self.host.window, HINT_TIMER_ID, HINT_DELAY_MS, None) != 0
                {
                    self.due_at =
                        Some(Instant::now() + Duration::from_millis(HINT_DELAY_MS.into()));
                    self.evidence.hint_show_request_count += 1;
                    self.evidence_dirty = true;
                }
                Some(0)
            }
            HINT_HOST_DISMISS if wparam as u64 == self.generation => {
                let dismissal = self
                    .queued_dismissal
                    .take()
                    .unwrap_or(HintDismissal::Rebuild);
                self.deactivate_on_host(dismissal);
                Some(0)
            }
            WM_TIMER if wparam == HINT_TIMER_ID => {
                self.show_pending_on_host();
                Some(0)
            }
            WM_NOTIFY => self.handle_tooltip_notify(lparam),
            _ => None,
        }
    }

    /// Hides a pending or visible hint before the backing menu is replaced.
    /// # Safety
    /// Must be called on the tray UI thread.
    pub unsafe fn dismiss(&mut self) {
        self.queue_dismiss(HintDismissal::Rebuild);
    }

    /// Hides a pending or visible hint when the presentation preference has
    /// changed. `disabled` is supplied by the render model, never inferred
    /// from visible text.
    /// # Safety
    /// Must be called on the tray UI thread.
    pub unsafe fn dismiss_for_menu_rebuild(&mut self, disabled: bool) {
        self.queue_dismiss(if disabled {
            HintDismissal::Disabled
        } else {
            HintDismissal::Rebuild
        });
    }

    /// # Safety
    ///
    /// Must run before the owner tray window is destroyed and on its UI thread.
    pub unsafe fn dispose(&mut self) {
        self.pending = None;
        self.deactivate_on_host(HintDismissal::Shutdown);
        self.host.dispose();
    }

    unsafe fn handle_tooltip_notify(&mut self, lparam: LPARAM) -> Option<LRESULT> {
        let tooltip = self.host.tooltip.as_mut()?;
        let result = tooltip.handle_custom_draw(lparam)?;
        self.evidence.tooltip_nm_customdraw_received = tooltip.custom_draw_count > 0;
        self.evidence.hint_paint_count = tooltip.custom_draw_count;
        self.evidence_dirty = true;
        Some(result)
    }
}

unsafe extern "system" fn hint_host_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        if let Some(create) = (lparam as *const CREATESTRUCTW).as_ref() {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
    }
    let controller = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MenuHintTooltip;
    if !controller.is_null() {
        if let Some(result) = (*controller).handle_host_message(hwnd, message, wparam, lparam) {
            return result;
        }
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

#[derive(Clone, Copy)]
enum HintDismissal {
    SelectionChanged,
    MenuClosed,
    Disabled,
    Rebuild,
    Shutdown,
}

impl ThemedMenu {
    fn empty() -> Self {
        Self {
            items: Vec::new(),
            brushes: Vec::new(),
            font: std::ptr::null_mut(),
            dpi: 96,
            metrics: MenuDpiMetrics {
                item_height: ITEM_HEIGHT_LOGICAL as u32,
                separator_height: SEPARATOR_HEIGHT_LOGICAL as u32,
                item_width: ITEM_WIDTH_LOGICAL as u32,
                check_gutter: CHECK_GUTTER_LOGICAL,
                submenu_gutter: SUBMENU_GUTTER_LOGICAL,
                item_inset: ITEM_INSET_LOGICAL,
            },
            palette: TrayThemePalette::for_effective(EffectiveTheme::Light),
            registry: NativeMenuHintRegistry::default(),
            root_menu: std::ptr::null_mut(),
        }
    }
}

impl Drop for ThemedMenu {
    fn drop(&mut self) {
        for brush in self.brushes.drain(..) {
            unsafe {
                DeleteObject(brush as _);
            }
        }
        if !self.font.is_null() {
            unsafe {
                DeleteObject(self.font as _);
            }
            self.font = std::ptr::null_mut();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnerDrawKind {
    Item,
    Separator,
    Submenu,
}

struct OwnerDrawItem {
    label: Vec<u16>,
    palette: TrayThemePalette,
    kind: OwnerDrawKind,
    font: HFONT,
    metrics: MenuDpiMetrics,
}

/// Installs the owner-drawn flags and data on a freshly built `muda` popup.
/// Callers must retain the returned value until they first detach the matching
/// menu from the tray icon.
/// # Safety
///
/// `dpi_window` must be a live UI-thread window handle that owns the popup's
/// presentation context.
pub unsafe fn theme_popup_menu(
    menu: &Menu,
    render: &TrayRender,
    effective_theme: EffectiveTheme,
    dpi_window: HWND,
) -> Result<ThemedMenu, String> {
    let palette =
        TrayThemePalette::for_effective(effective_theme_for_accessibility(effective_theme));
    let mut themed = ThemedMenu::empty();
    themed.palette = palette;
    themed.root_menu = menu.hpopupmenu() as HMENU;
    let resources = menu_dpi_resources(dpi_window)?;
    themed.font = resources.font;
    themed.dpi = resources.dpi;
    themed.metrics = resources.metrics;
    decorate_menu(
        menu.hpopupmenu() as HMENU,
        &render.entries,
        palette,
        resources.metrics,
        resources.font,
        &mut themed,
    )?;
    Ok(themed)
}

/// The `tray-icon` HWND is public and remains valid with its `TrayIcon`.
/// Adding this documented common-controls subclass lets the owning UI thread
/// receive the owner-draw messages while preserving tray-icon/muda dispatch.
/// # Safety
///
/// `hwnd` must be the live tray-icon window handle and this function must be
/// called on the tray UI thread.
pub unsafe fn install_owner_draw_hook(
    hwnd: HWND,
    tooltip: *mut MenuHintTooltip,
) -> Result<(), String> {
    let installed = SetWindowSubclass(
        hwnd,
        Some(owner_draw_proc),
        THEME_SUBCLASS_ID,
        tooltip as usize,
    );
    if installed == 0 {
        return Err("could not install the Windows tray owner-draw handler".to_owned());
    }
    Ok(())
}

/// # Safety
///
/// `hwnd` must remain valid and belong to the UI thread that installed this
/// subclass.
pub unsafe fn remove_owner_draw_hook(hwnd: HWND) {
    RemoveWindowSubclass(hwnd, Some(owner_draw_proc), THEME_SUBCLASS_ID);
}

/// Queues a development-only exercise through the exact tray HWND subclass.
/// It never opens, controls, or observes an official runner; it exists solely
/// to validate the documented menu-notification and tooltip path before a
/// visual Owner check.
/// # Safety
/// `hwnd` and `themed` must be live on the tray UI thread.
pub unsafe fn queue_development_hint_exercise(
    hwnd: HWND,
    themed: &ThemedMenu,
) -> Result<(), String> {
    if themed.root_menu.is_null() {
        return Err("development hint exercise did not have a native root menu".to_owned());
    }
    if PostMessageW(hwnd, WM_INITMENUPOPUP, themed.root_menu as usize, 0) == 0 {
        return Err("could not queue native menu initialization evidence".to_owned());
    }
    for key in [
        TrayHelpKey::Zen,
        TrayHelpKey::Mode,
        TrayHelpKey::ModeChoice(crate::UserMode::Auto),
        TrayHelpKey::Probes,
        TrayHelpKey::Probe(crate::ProbeId::new("steam-game").expect("static probe id")),
    ] {
        if let Some((command, _)) = themed
            .registry
            .command_items
            .iter()
            .find(|(_, registered)| **registered == key)
        {
            if PostMessageW(
                hwnd,
                WM_MENUSELECT,
                *command as usize,
                themed.root_menu as LPARAM,
            ) == 0
            {
                return Err("could not queue native command-menu selection evidence".to_owned());
            }
            continue;
        }
        if let Some((parent, index, _)) = themed
            .registry
            .submenu_locations
            .iter()
            .find(|(_, _, registered)| *registered == key)
        {
            let wparam = ((*index as usize) & 0xffff) | ((MF_POPUP as usize) << 16);
            if PostMessageW(hwnd, WM_MENUSELECT, wparam, *parent as LPARAM) == 0 {
                return Err("could not queue native submenu selection evidence".to_owned());
            }
        }
    }
    if SetTimer(
        hwnd,
        HINT_EXERCISE_CLOSE_TIMER_ID,
        HINT_DELAY_MS + 1_000,
        None,
    ) == 0
    {
        return Err("could not schedule development menu-close evidence".to_owned());
    }
    Ok(())
}

fn effective_theme_for_accessibility(theme: EffectiveTheme) -> EffectiveTheme {
    if high_contrast_enabled() {
        EffectiveTheme::SystemAccessible
    } else {
        theme
    }
}

fn high_contrast_enabled() -> bool {
    let mut settings = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            settings.cbSize,
            (&mut settings as *mut HIGHCONTRASTW).cast(),
            0,
        ) != 0
            && settings.dwFlags & HCF_HIGHCONTRASTON != 0
    }
}

unsafe fn decorate_menu(
    hmenu: HMENU,
    entries: &[TrayMenuEntry],
    palette: TrayThemePalette,
    metrics: MenuDpiMetrics,
    font: HFONT,
    themed: &mut ThemedMenu,
) -> Result<(), String> {
    let count = GetMenuItemCount(hmenu);
    if count < 0 || count as usize != entries.len() {
        return Err("native popup menu did not match its stable render model".to_owned());
    }

    let brush = CreateSolidBrush(palette.surface);
    if brush.is_null() {
        return Err("could not create a Windows tray menu background brush".to_owned());
    }
    let info = MENUINFO {
        cbSize: size_of::<MENUINFO>() as u32,
        fMask: MIM_BACKGROUND,
        hbrBack: brush,
        ..Default::default()
    };
    if SetMenuInfo(hmenu, &info) == 0 {
        DeleteObject(brush as _);
        return Err("could not apply the Windows tray menu background".to_owned());
    }
    themed.brushes.push(brush);

    for (position, entry) in entries.iter().enumerate() {
        let (label, kind, child_entries) = match entry {
            TrayMenuEntry::Separator => (String::new(), OwnerDrawKind::Separator, None),
            TrayMenuEntry::Item(item) => (item.label.clone(), OwnerDrawKind::Item, None),
            TrayMenuEntry::Submenu { label, entries, .. } => (
                label.clone(),
                OwnerDrawKind::Submenu,
                Some(entries.as_slice()),
            ),
        };
        let item = Box::new(OwnerDrawItem {
            label: label.encode_utf16().collect(),
            palette,
            kind,
            font,
            metrics,
        });
        let item_data = item.as_ref() as *const OwnerDrawItem as usize;
        let f_type = match kind {
            OwnerDrawKind::Separator => MFT_OWNERDRAW | MFT_SEPARATOR,
            OwnerDrawKind::Item | OwnerDrawKind::Submenu => MFT_OWNERDRAW,
        };
        let info = MENUITEMINFOW {
            cbSize: size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_FTYPE | MIIM_DATA,
            fType: f_type,
            dwItemData: item_data,
            ..Default::default()
        };
        if SetMenuItemInfoW(hmenu, position as u32, 1, &info) == 0 {
            return Err("could not mark a Windows tray menu item owner-drawn".to_owned());
        }
        themed.items.push(item);

        let help_key = hint_key_for_entry(entry);
        let mut native_id = MENUITEMINFOW {
            cbSize: size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_ID,
            ..Default::default()
        };
        if GetMenuItemInfoW(hmenu, position as u32, 1, &mut native_id) == 0 {
            return Err("could not read a Windows tray menu command identifier".to_owned());
        }
        if let (Some(key), OwnerDrawKind::Item) = (help_key.clone(), kind) {
            themed.registry.command_items.insert(native_id.wID, key);
            themed.registry.command_locations.insert(
                native_id.wID,
                NativeMenuItemLocation {
                    menu: hmenu,
                    position: position as u32,
                },
            );
        }

        if let Some(entries) = child_entries {
            let child = GetSubMenu(hmenu, position as i32);
            if child.is_null() {
                return Err("a RunnerMesh tray submenu did not have a native popup".to_owned());
            }
            if let Some(key) = help_key {
                themed.registry.submenus.insert(child as isize, key.clone());
                themed
                    .registry
                    .submenu_locations
                    .push((hmenu, position as u32, key));
            }
            decorate_menu(child, entries, palette, metrics, font, themed)?;
        }
    }
    Ok(())
}

fn hint_key_for_entry(entry: &TrayMenuEntry) -> Option<TrayHelpKey> {
    match entry {
        TrayMenuEntry::Item(item) => TrayHelpKey::from_menu_id(&item.id),
        TrayMenuEntry::Submenu { id, .. } if id == "control.mode" => Some(TrayHelpKey::Mode),
        TrayMenuEntry::Submenu { id, .. } if id == "control.probes" => Some(TrayHelpKey::Probes),
        TrayMenuEntry::Separator | TrayMenuEntry::Submenu { .. } => None,
    }
}

unsafe extern "system" fn owner_draw_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    reference_data: usize,
) -> LRESULT {
    match message {
        WM_INITMENUPOPUP => {
            if reference_data != 0 {
                let tooltip = &mut *(reference_data as *mut MenuHintTooltip);
                tooltip.evidence.init_menu_popup_count += 1;
                tooltip.evidence_dirty = true;
            }
        }
        WM_MENUSELECT => {
            if reference_data != 0 {
                let tooltip = &mut *(reference_data as *mut MenuHintTooltip);
                let flags = ((wparam >> 16) & 0xffff) as u32;
                if flags == 0xffff && lparam == 0 {
                    tooltip.evidence.menu_close_count += 1;
                    tooltip.evidence_dirty = true;
                    tooltip.queue_dismiss(HintDismissal::MenuClosed);
                } else {
                    tooltip.select(resolve_menu_selection(&tooltip.registry, wparam, lparam));
                }
            }
        }
        WM_UNINITMENUPOPUP => {
            if reference_data != 0 {
                let tooltip = &mut *(reference_data as *mut MenuHintTooltip);
                tooltip.evidence.uninit_menu_popup_count += 1;
                tooltip.evidence_dirty = true;
                tooltip.queue_dismiss(HintDismissal::MenuClosed);
            }
        }
        WM_DPICHANGED => {
            if reference_data != 0 {
                let tooltip = &mut *(reference_data as *mut MenuHintTooltip);
                tooltip.note_dpi_changed();
            }
        }
        WM_TIMER if wparam == HINT_EXERCISE_CLOSE_TIMER_ID => {
            KillTimer(hwnd, HINT_EXERCISE_CLOSE_TIMER_ID);
            if reference_data != 0 {
                let tooltip = &mut *(reference_data as *mut MenuHintTooltip);
                tooltip.evidence.uninit_menu_popup_count += 1;
                tooltip.evidence.menu_close_count += 1;
                tooltip.evidence_dirty = true;
                tooltip.queue_dismiss(HintDismissal::MenuClosed);
                return 0;
            }
        }
        WM_MEASUREITEM => {
            let measure = (lparam as *mut MEASUREITEMSTRUCT).as_mut();
            if let Some(measure) = measure {
                if measure.CtlType == ODT_MENU && measure.itemData != 0 {
                    let item = &*(measure.itemData as *const OwnerDrawItem);
                    measure.itemHeight = match item.kind {
                        OwnerDrawKind::Separator => item.metrics.separator_height,
                        OwnerDrawKind::Item | OwnerDrawKind::Submenu => item.metrics.item_height,
                    };
                    measure.itemWidth = item.metrics.item_width;
                    return 1;
                }
            }
        }
        WM_DRAWITEM => {
            let draw = (lparam as *const DRAWITEMSTRUCT).as_ref();
            if let Some(draw) = draw {
                if draw.CtlType == ODT_MENU && draw.itemData != 0 {
                    draw_item(draw, &*(draw.itemData as *const OwnerDrawItem));
                    return 1;
                }
            }
        }
        _ => {}
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

unsafe fn resolve_menu_selection(
    registry: &NativeMenuHintRegistry,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<ResolvedMenuSelection> {
    let selection = parse_menu_select(wparam, lparam);
    let MenuSelect::Item { item, flags, menu } = selection else {
        return None;
    };
    if flags & MF_POPUP != 0 {
        let submenu = GetSubMenu(menu, item as i32);
        registry
            .submenus
            .get(&(submenu as isize))
            .cloned()
            .map(|key| ResolvedMenuSelection {
                key,
                location: Some(NativeMenuItemLocation {
                    menu,
                    position: item,
                }),
            })
    } else {
        registry
            .command_items
            .get(&item)
            .cloned()
            .map(|key| ResolvedMenuSelection {
                key,
                location: registry.command_locations.get(&item).copied(),
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuSelect {
    Closed,
    Item { item: u32, flags: u32, menu: HMENU },
}

fn parse_menu_select(wparam: WPARAM, lparam: LPARAM) -> MenuSelect {
    let flags = ((wparam >> 16) & 0xffff) as u32;
    if flags == 0xffff && lparam == 0 {
        MenuSelect::Closed
    } else {
        MenuSelect::Item {
            item: (wparam & 0xffff) as u32,
            flags,
            menu: lparam as HMENU,
        }
    }
}

fn mark_hint_key(evidence: &mut NativeHintEvidence, key: &TrayHelpKey) {
    match key {
        TrayHelpKey::Zen => evidence.zen_hint_key_resolved = true,
        TrayHelpKey::Mode => evidence.mode_submenu_hint_key_resolved = true,
        TrayHelpKey::ModeChoice(_) => evidence.mode_child_hint_key_resolved = true,
        TrayHelpKey::Probes => evidence.probes_submenu_hint_key_resolved = true,
        TrayHelpKey::Probe(_) => evidence.probe_child_hint_key_resolved = true,
    }
}

unsafe fn draw_item(draw: &DRAWITEMSTRUCT, item: &OwnerDrawItem) {
    let selected = draw.itemState & ODS_SELECTED != 0;
    let disabled = draw.itemState & ODS_DISABLED != 0;
    let palette = item.palette;
    let background = if selected {
        palette.selected_surface
    } else {
        palette.surface
    };
    fill(draw.hDC, &draw.rcItem, background);

    if item.kind == OwnerDrawKind::Separator {
        let y = (draw.rcItem.top + draw.rcItem.bottom) / 2;
        let separator = RECT {
            left: draw.rcItem.left + item.metrics.check_gutter,
            top: y,
            right: draw.rcItem.right - item.metrics.item_inset,
            bottom: y + 1,
        };
        fill(draw.hDC, &separator, palette.separator);
        return;
    }

    let text_color = if disabled {
        palette.disabled_text
    } else if selected {
        palette.selected_text
    } else {
        palette.text
    };
    SetTextColor(draw.hDC, text_color);
    SetBkMode(draw.hDC, TRANSPARENT as i32);
    let previous_font = if item.font.is_null() {
        std::ptr::null_mut()
    } else {
        SelectObject(draw.hDC, item.font as _)
    };

    if draw.itemState & ODS_CHECKED != 0 {
        draw_label(
            draw.hDC,
            "✓",
            RECT {
                left: draw.rcItem.left + item.metrics.item_inset,
                top: draw.rcItem.top,
                right: draw.rcItem.left + item.metrics.check_gutter,
                bottom: draw.rcItem.bottom,
            },
        );
    }

    let mut text_rect = draw.rcItem;
    text_rect.left += item.metrics.check_gutter;
    text_rect.right -= if item.kind == OwnerDrawKind::Submenu {
        item.metrics.submenu_gutter
    } else {
        item.metrics.item_inset
    };
    DrawTextW(
        draw.hDC,
        item.label.as_ptr(),
        item.label.len() as i32,
        &mut text_rect,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );

    if item.kind == OwnerDrawKind::Submenu {
        draw_label(
            draw.hDC,
            "›",
            RECT {
                left: draw.rcItem.right - item.metrics.submenu_gutter,
                top: draw.rcItem.top,
                right: draw.rcItem.right - item.metrics.item_inset,
                bottom: draw.rcItem.bottom,
            },
        );
    }
    if !previous_font.is_null() {
        SelectObject(draw.hDC, previous_font);
    }
}

unsafe fn fill(hdc: windows_sys::Win32::Graphics::Gdi::HDC, rect: &RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    if !brush.is_null() {
        FillRect(hdc, rect, brush);
        DeleteObject(brush as _);
    }
}

unsafe fn draw_label(hdc: windows_sys::Win32::Graphics::Gdi::HDC, label: &str, mut rect: RECT) {
    let wide: Vec<u16> = label.encode_utf16().collect();
    DrawTextW(
        hdc,
        wide.as_ptr(),
        wide.len() as i32,
        &mut rect,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

#[cfg(test)]
mod tests {
    use super::{
        choose_tooltip_placement, clamp_to_work_area, parse_menu_select, rect_inside,
        rects_intersect, scale_logical_px, EffectiveTheme, MenuSelect, TooltipPlacementDirection,
        TooltipSize, TrayThemePalette,
    };
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::MF_POPUP;

    #[test]
    fn explicit_light_and_dark_use_distinct_deterministic_palettes() {
        let light = TrayThemePalette::for_effective(EffectiveTheme::Light);
        let dark = TrayThemePalette::for_effective(EffectiveTheme::Dark);
        assert_ne!(light.surface, dark.surface);
        assert_ne!(light.text, dark.text);
        assert_ne!(light.selected_surface, dark.selected_surface);
    }

    #[test]
    fn menu_select_preserves_documented_command_popup_and_close_semantics() {
        let command = parse_menu_select(42, 1);
        assert!(matches!(
            command,
            MenuSelect::Item {
                item: 42,
                flags: 0,
                ..
            }
        ));
        let popup = parse_menu_select(((MF_POPUP as usize) << 16) | 3, 1);
        assert!(matches!(
            popup,
            MenuSelect::Item {
                item: 3,
                flags,
                ..
            } if flags & MF_POPUP != 0
        ));
        assert_eq!(parse_menu_select(usize::MAX, 0), MenuSelect::Closed);
    }

    #[test]
    fn native_hint_popup_positioning_preserves_negative_monitor_coordinates() {
        assert_eq!(clamp_to_work_area(-1_900, -1_920, 0, 200), -1_900);
        assert_eq!(clamp_to_work_area(50, -1_920, 0, 200), -200);
        assert_eq!(clamp_to_work_area(-2_000, -1_920, 0, 200), -1_920);
    }

    #[test]
    fn native_hint_popup_requires_work_area_intersection() {
        let work = RECT {
            left: -1_920,
            top: 0,
            right: 0,
            bottom: 1_080,
        };
        let visible = RECT {
            left: -40,
            top: 10,
            right: 160,
            bottom: 80,
        };
        let hidden = RECT {
            left: 10,
            top: 10,
            right: 160,
            bottom: 80,
        };
        assert!(rects_intersect(visible, work));
        assert!(!rects_intersect(hidden, work));
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    fn size(width: i32, height: i32) -> TooltipSize {
        TooltipSize { width, height }
    }

    #[test]
    fn adaptive_tooltip_placement_prefers_right_with_ample_room() {
        let placement = choose_tooltip_placement(
            rect(100, 100, 180, 130),
            size(240, 60),
            rect(0, 0, 1_000, 800),
            10,
        );
        assert_eq!(placement.direction, TooltipPlacementDirection::Right);
        assert!(rect_inside(placement.rect, rect(0, 0, 1_000, 800)));
        assert!(!placement.clamped);
    }

    #[test]
    fn adaptive_tooltip_placement_uses_left_at_right_edge() {
        let placement = choose_tooltip_placement(
            rect(850, 100, 900, 130),
            size(240, 60),
            rect(0, 0, 1_000, 800),
            10,
        );
        assert_eq!(placement.direction, TooltipPlacementDirection::Left);
        assert!(rect_inside(placement.rect, rect(0, 0, 1_000, 800)));
    }

    #[test]
    fn adaptive_tooltip_placement_uses_right_at_left_edge() {
        let placement = choose_tooltip_placement(
            rect(5, 100, 55, 130),
            size(240, 60),
            rect(0, 0, 1_000, 800),
            10,
        );
        assert_eq!(placement.direction, TooltipPlacementDirection::Right);
        assert!(rect_inside(placement.rect, rect(0, 0, 1_000, 800)));
    }

    #[test]
    fn adaptive_tooltip_placement_uses_below_when_horizontal_sides_do_not_fit() {
        let placement = choose_tooltip_placement(
            rect(100, 100, 200, 130),
            size(300, 60),
            rect(0, 0, 400, 800),
            10,
        );
        assert_eq!(placement.direction, TooltipPlacementDirection::Below);
        assert!(rect_inside(placement.rect, rect(0, 0, 400, 800)));
    }

    #[test]
    fn adaptive_tooltip_placement_uses_above_when_bottom_is_constrained() {
        let placement = choose_tooltip_placement(
            rect(100, 250, 200, 280),
            size(300, 60),
            rect(0, 0, 400, 300),
            10,
        );
        assert_eq!(placement.direction, TooltipPlacementDirection::Above);
        assert!(rect_inside(placement.rect, rect(0, 0, 400, 300)));
    }

    #[test]
    fn adaptive_tooltip_placement_preserves_negative_secondary_coordinates() {
        let work = rect(-1_920, 0, 0, 1_080);
        let placement =
            choose_tooltip_placement(rect(-120, 100, -70, 130), size(240, 60), work, 10);
        assert_eq!(placement.direction, TooltipPlacementDirection::Left);
        assert!(rect_inside(placement.rect, work));
        assert!(placement.rect.left < 0);
    }

    #[test]
    fn adaptive_tooltip_placement_respects_taskbar_reduced_work_area() {
        let work = rect(0, 0, 400, 760);
        let placement = choose_tooltip_placement(rect(100, 710, 200, 740), size(300, 60), work, 10);
        assert_eq!(placement.direction, TooltipPlacementDirection::Above);
        assert!(rect_inside(placement.rect, work));
        assert!(placement.rect.bottom <= 760);
    }

    #[test]
    fn adaptive_tooltip_placement_clamps_nearly_work_area_sized_bubble() {
        let work = rect(0, 0, 1_000, 700);
        let placement =
            choose_tooltip_placement(rect(800, 300, 850, 330), size(950, 120), work, 10);
        assert!(placement.clamped);
        assert!(rect_inside(placement.rect, work));
        assert_eq!(placement.rect.left, 0);
    }

    #[test]
    fn logical_menu_metrics_scale_exactly_once_at_supported_dpis() {
        let cases = [(96, 10), (120, 13), (144, 15), (192, 20), (240, 25)];
        let mut prior = 0;
        for (dpi, expected) in cases {
            let scaled = scale_logical_px(10, dpi);
            assert_eq!(scaled, expected, "unexpected scale at {dpi} DPI");
            assert!(scaled >= prior);
            prior = scaled;
        }
        assert_eq!(scale_logical_px(26, 192), 52);
    }

    #[test]
    fn negative_monitor_coordinates_remain_independent_from_dpi_scaling() {
        let work = rect(-1_920, 0, 0, 1_080);
        for dpi in [96, 120, 144, 192, 240] {
            let placement = choose_tooltip_placement(
                rect(-120, 100, -70, 130),
                size(scale_logical_px(200, dpi), scale_logical_px(48, dpi)),
                work,
                scale_logical_px(10, dpi),
            );
            assert!(rect_inside(placement.rect, work), "{dpi} DPI");
            assert!(placement.rect.left < 0, "{dpi} DPI");
        }
    }
}
