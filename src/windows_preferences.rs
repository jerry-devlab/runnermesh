//! Documented Windows current-user preference observation.
//!
//! This module observes presentation facts only. It never writes registry
//! state, changes Windows appearance, or rewrites RunnerMesh preferences.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use windows::{
    core::{Error as WindowsError, IInspectable},
    Foundation::TypedEventHandler,
    UI::ViewManagement::{UIColorType, UISettings},
};
use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

use crate::{EffectiveLocale, EffectiveTheme, SystemPreferences};

/// Reads supported current-user Windows presentation sources once. Errors are
/// intentionally explicit so an Agent never silently claims a system value it
/// could not observe.
pub fn observe_system_preferences() -> Result<SystemPreferences, String> {
    Ok(SystemPreferences {
        theme: observe_system_theme()?,
        locale: observe_system_locale()?,
    })
}

/// Maps a documented UISettings foreground color to an app palette: a light
/// foreground is rendered against a dark application surface and vice versa.
pub fn theme_from_foreground(red: u8, green: u8, blue: u8) -> EffectiveTheme {
    // Relative luminance keeps the direction deterministic without treating an
    // undocumented registry value as authoritative.
    let brightness = 0.2126 * f64::from(red) + 0.7152 * f64::from(green) + 0.0722 * f64::from(blue);
    if brightness >= 128.0 {
        EffectiveTheme::Dark
    } else {
        EffectiveTheme::Light
    }
}

/// Resolves the first current-user UI-language entry using the v0.1 supported
/// locale set. The system UI language is intentionally distinct from keyboard,
/// region, and installation locale.
pub fn locale_from_user_languages(languages: &[String]) -> EffectiveLocale {
    let first = languages.first().map(String::as_str).unwrap_or_default();
    let normalized = first.replace('_', "-").to_ascii_lowercase();
    if normalized == "zh-cn" || normalized == "zh-hans" || normalized.starts_with("zh-") {
        EffectiveLocale::ZhCn
    } else {
        EffectiveLocale::EnUs
    }
}

fn observe_system_theme() -> Result<EffectiveTheme, String> {
    let settings = UISettings::new().map_err(windows_error)?;
    let color = settings
        .GetColorValue(UIColorType::Foreground)
        .map_err(windows_error)?;
    Ok(theme_from_foreground(color.R, color.G, color.B))
}

fn observe_system_locale() -> Result<EffectiveLocale, String> {
    let languages = user_preferred_ui_languages()?;
    Ok(locale_from_user_languages(&languages))
}

fn user_preferred_ui_languages() -> Result<Vec<String>, String> {
    unsafe {
        let mut count = 0_u32;
        let mut length = 0_u32;
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
        ) == 0
        {
            return Err("GetUserPreferredUILanguages size query failed".to_owned());
        }
        let mut buffer = vec![0_u16; length as usize];
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut length,
        ) == 0
        {
            return Err("GetUserPreferredUILanguages read failed".to_owned());
        }

        Ok(buffer
            .split(|code_unit| *code_unit == 0)
            .filter(|entry| !entry.is_empty())
            .map(String::from_utf16_lossy)
            .collect())
    }
}

/// Keeps the documented UISettings color-change subscription alive. Its event
/// handler only marks work pending; the tray UI thread performs all native menu
/// mutation when it later consumes the flag.
pub struct SystemThemeChangeMonitor {
    settings: UISettings,
    token: i64,
    changed: Arc<AtomicBool>,
}

impl SystemThemeChangeMonitor {
    pub fn new() -> Result<Self, String> {
        let settings = UISettings::new().map_err(windows_error)?;
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_event = Arc::clone(&changed);
        let handler = TypedEventHandler::<UISettings, IInspectable>::new(move |_, _| {
            changed_for_event.store(true, Ordering::Release);
            Ok(())
        });
        let token = settings
            .ColorValuesChanged(&handler)
            .map_err(windows_error)?;
        Ok(Self {
            settings,
            token,
            changed,
        })
    }

    pub fn take_change(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }
}

impl Drop for SystemThemeChangeMonitor {
    fn drop(&mut self) {
        let _ = self.settings.RemoveColorValuesChanged(self.token);
    }
}

fn windows_error(error: WindowsError) -> String {
    format!("Windows presentation API failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::{locale_from_user_languages, theme_from_foreground};
    use crate::{
        EffectiveLocale, EffectiveTheme, LanguagePreference, SystemPreferences, ThemePreference,
        UiPreferences,
    };

    #[test]
    fn foreground_direction_resolves_documented_app_palette() {
        assert_eq!(theme_from_foreground(240, 240, 240), EffectiveTheme::Dark);
        assert_eq!(theme_from_foreground(24, 24, 24), EffectiveTheme::Light);
    }

    #[test]
    fn current_user_ui_language_uses_first_preference() {
        assert_eq!(
            locale_from_user_languages(&["zh-Hans-SG".to_owned(), "en-US".to_owned()]),
            EffectiveLocale::ZhCn
        );
        assert_eq!(
            locale_from_user_languages(&["en-US".to_owned(), "zh-CN".to_owned()]),
            EffectiveLocale::EnUs
        );
        assert_eq!(
            locale_from_user_languages(&["fr-FR".to_owned()]),
            EffectiveLocale::EnUs
        );
    }

    #[test]
    fn persisted_system_preferences_resolve_without_rewriting_system() {
        let preferences = UiPreferences::default();
        let effective = preferences.resolve(SystemPreferences {
            theme: EffectiveTheme::Dark,
            locale: EffectiveLocale::ZhCn,
        });
        assert_eq!(preferences.theme, ThemePreference::System);
        assert_eq!(preferences.language, LanguagePreference::System);
        assert_eq!(effective.theme, EffectiveTheme::Dark);
        assert_eq!(effective.locale, EffectiveLocale::ZhCn);
    }

    #[test]
    fn explicit_preferences_override_system_sources() {
        let preferences = UiPreferences {
            theme: ThemePreference::Light,
            language: LanguagePreference::EnUs,
            menu_hints_enabled: true,
        };
        let effective = preferences.resolve(SystemPreferences {
            theme: EffectiveTheme::Dark,
            locale: EffectiveLocale::ZhCn,
        });
        assert_eq!(effective.theme, EffectiveTheme::Light);
        assert_eq!(effective.locale, EffectiveLocale::EnUs);
    }
}
