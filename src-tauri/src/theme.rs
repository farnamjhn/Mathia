//! Window appearance sync so desktop light/dark overrides tint only the app chrome,
//! never the compiled HTML inside the preview `WKWebView` iframe.

use tauri::{Theme, WebviewWindow, Window};

/// User-selected desktop app theme mode (`system` follows the OS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    /// Parses a frontend `<meta name="color-scheme">` value into a `ThemeChoice`.
    pub fn parse(raw: Option<&str>) -> Result<Self, String> {
        match raw.map(str::trim) {
            None | Some("") | Some("system") | Some("light dark") => Ok(Self::System),
            Some("light") => Ok(Self::Light),
            Some("dark") => Ok(Self::Dark),
            Some(other) => Err(format!("unsupported theme choice: {other}")),
        }
    }
}

/// Applies the requested appearance to the native window frame and returns the OS
/// system theme name (`"light"` or `"dark"`).
pub fn apply(window: &WebviewWindow, choice: ThemeChoice) -> Result<&'static str, String> {
    let os_theme = system_theme_name(window);
    apply_platform(window, choice)?;
    Ok(os_theme)
}

/// Returns `"dark"` or `"light"` reflecting the OS appearance.
pub fn system_theme_name(window: &WebviewWindow) -> &'static str {
    match window.theme() {
        Ok(Theme::Dark) => "dark",
        _ => "light",
    }
}

/// Keeps `contentView`'s OS appearance current when the OS theme changes while an
/// explicit app theme override is pinned on the window frame.
pub fn sync_os_content_appearance(window: &Window, os_theme: Theme) {
    #[cfg(target_os = "macos")]
    macos::sync_os_content_appearance(window, os_theme);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, os_theme);
}

#[cfg(target_os = "macos")]
fn apply_platform(window: &WebviewWindow, choice: ThemeChoice) -> Result<(), String> {
    let win = window.clone();
    window
        .run_on_main_thread(move || macos::set_ns_window_appearance(&win, choice))
        .map_err(|e| format!("could not schedule macOS appearance update: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn apply_platform(_window: &WebviewWindow, _choice: ThemeChoice) -> Result<(), String> {
    // Non-macOS backends do not split window frame vs WebView content appearance;
    // keep WebView CSS `<meta name="color-scheme">` independent of the HTML iframe.
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::ThemeChoice;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSApplication, NSWindow,
    };
    use objc2_foundation::MainThreadMarker;
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use tauri::{Theme, WebviewWindow, Window};

    fn named_appearance(choice: ThemeChoice) -> Option<Retained<NSAppearance>> {
        match choice {
            ThemeChoice::System => None,
            // SAFETY: static Cocoa appearance names are valid for the lifetime of the process.
            ThemeChoice::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
            ThemeChoice::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
        }
    }

    /// Invokes `f` with the live `NSWindow` reference on the AppKit main thread.
    fn with_ns_window(raw_ptr: *mut c_void, f: impl FnOnce(MainThreadMarker, &NSWindow)) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(ptr) = NonNull::new(raw_ptr.cast::<NSWindow>()) else {
            return;
        };
        // SAFETY: Caller verified execution on the AppKit main thread and holds a live
        // Tauri `Window`/`WebviewWindow` whose `ns_window()` pointer remains valid.
        let ns_window = unsafe { ptr.as_ref() };
        f(mtm, ns_window);
    }

    pub(super) fn set_ns_window_appearance(window: &WebviewWindow, choice: ThemeChoice) {
        let Ok(raw_ptr) = window.ns_window() else {
            return;
        };
        with_ns_window(raw_ptr, |mtm, ns_window| match choice {
            ThemeChoice::System => {
                // Clear the window override before resetting `contentView` so `WKWebView`
                // never momentarily inherits an overridden window appearance.
                ns_window.setAppearance(None);
                if let Some(content_view) = ns_window.contentView() {
                    content_view.setAppearance(None);
                }
            }
            ThemeChoice::Light | ThemeChoice::Dark => {
                // Pin `contentView` to the OS appearance before setting the window override so
                // `(prefers-color-scheme)` inside the preview `WKWebView` iframe stays untouched.
                let os_appearance = NSApplication::sharedApplication(mtm).effectiveAppearance();
                if let Some(content_view) = ns_window.contentView() {
                    content_view.setAppearance(Some(&os_appearance));
                }
                ns_window.setAppearance(named_appearance(choice).as_deref());
            }
        });
    }

    pub(super) fn sync_os_content_appearance(window: &Window, os_theme: Theme) {
        let Ok(raw_ptr) = window.ns_window() else {
            return;
        };
        with_ns_window(raw_ptr, |_mtm, ns_window| {
            if ns_window.appearance().is_none() {
                return;
            }
            if let Some(content_view) = ns_window.contentView() {
                let appearance = named_appearance(match os_theme {
                    Theme::Dark => ThemeChoice::Dark,
                    _ => ThemeChoice::Light,
                });
                content_view.setAppearance(appearance.as_deref());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_theme_choices() {
        assert_eq!(ThemeChoice::parse(None).unwrap(), ThemeChoice::System);
        assert_eq!(ThemeChoice::parse(Some("")).unwrap(), ThemeChoice::System);
        assert_eq!(
            ThemeChoice::parse(Some("light dark")).unwrap(),
            ThemeChoice::System
        );
        assert_eq!(
            ThemeChoice::parse(Some("system")).unwrap(),
            ThemeChoice::System
        );
        assert_eq!(
            ThemeChoice::parse(Some("light")).unwrap(),
            ThemeChoice::Light
        );
        assert_eq!(ThemeChoice::parse(Some("dark")).unwrap(), ThemeChoice::Dark);
    }

    #[test]
    fn rejects_unknown_theme_choices() {
        assert!(ThemeChoice::parse(Some("sepia")).is_err());
    }
}
