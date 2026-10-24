//! Tray glyph color on Linux.
//!
//! The bundled icon is a black template. macOS tints templates in the menu bar.
//! StatusNotifier hosts (COSMIC, GNOME AppIndicator, KDE) draw the pixmap unchanged,
//! so the same shape is painted white when the desktop prefers a dark appearance.
//! Attention/error badges are composited in `tray_icon` and must be redrawn on theme change.

use crate::tray_status::{self, TrayStatusState};
use std::future::poll_fn;
use std::pin::Pin;
use tauri::{AppHandle, Manager};
use zbus::export::futures_core::stream::Stream;
use zbus::{Connection, OwnedVariant, Variant};

const APPEARANCE_NS: &str = "org.freedesktop.appearance";
const COLOR_SCHEME_KEY: &str = "color-scheme";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorScheme {
    /// Portal value 0, or an unknown code. Fall back to the GTK theme name.
    NoPreference,
    Dark,
    Light,
}

#[zbus::proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait PortalSettings {
    fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<OwnedVariant>;

    #[zbus(signal)]
    fn setting_changed(&self, namespace: &str, key: &str, value: OwnedVariant) -> zbus::Result<()>;
}

/// `true` when the desktop color scheme is dark.
///
/// Uses the XDG settings portal (`color-scheme` 1). If the portal reports no
/// preference or is missing, falls back to GNOME/GTK theme settings, which
/// COSMIC also keeps in sync.
pub fn prefers_dark_ui() -> bool {
    match read_portal_scheme() {
        Ok(scheme) => scheme_is_dark(scheme),
        Err(err) => {
            tracing::debug!("Desktop color-scheme portal unavailable ({err}); using GTK theme");
            gtk_prefers_dark()
        }
    }
}

/// Keep the tray glyph in sync when the user switches light/dark.
pub fn follow_color_scheme(app: AppHandle) {
    if let Err(err) = std::thread::Builder::new()
        .name("tray-theme".into())
        .spawn(move || {
            if let Err(err) = zbus::block_on(watch(app)) {
                tracing::warn!("Tray color-scheme watcher stopped: {err}");
            }
        })
    {
        tracing::warn!("Failed to start tray color-scheme watcher: {err}");
    }
}

async fn watch(app: AppHandle) -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let proxy = PortalSettingsProxy::new(&connection).await?;
    let mut dark = scheme_is_dark(read_scheme(&proxy).await?);
    apply_dark(&app, dark);
    let mut signals = proxy.receive_setting_changed().await?;
    while let Some(signal) = poll_fn(|cx| Pin::new(&mut signals).poll_next(cx)).await {
        let args = signal.args()?;
        if *args.namespace() != APPEARANCE_NS || *args.key() != COLOR_SCHEME_KEY {
            continue;
        }
        let next = scheme_is_dark(scheme_from_value(args.value()));
        if next == dark {
            continue;
        }
        dark = next;
        apply_dark(&app, dark);
    }
    Ok(())
}

fn apply_dark(app: &AppHandle, dark: bool) {
    if let Some(state) = app.try_state::<TrayStatusState>() {
        state.set_on_dark(dark);
    }
    tray_status::reapply_icon(app);
}

fn read_portal_scheme() -> zbus::Result<ColorScheme> {
    zbus::block_on(async {
        let connection = Connection::session().await?;
        let proxy = PortalSettingsProxy::new(&connection).await?;
        read_scheme(&proxy).await
    })
}

async fn read_scheme(proxy: &PortalSettingsProxy<'_>) -> zbus::Result<ColorScheme> {
    let value = proxy.read_one(APPEARANCE_NS, COLOR_SCHEME_KEY).await?;
    Ok(scheme_from_value(&value))
}

fn scheme_from_value(value: &Variant<'_>) -> ColorScheme {
    match color_scheme_code(value) {
        Some(code) => scheme_from_code(code),
        None => ColorScheme::NoPreference,
    }
}

fn scheme_from_code(code: u32) -> ColorScheme {
    match code {
        1 => ColorScheme::Dark,
        2 => ColorScheme::Light,
        _ => ColorScheme::NoPreference,
    }
}

fn color_scheme_code(value: &Variant<'_>) -> Option<u32> {
    match value {
        Variant::U32(code) => Some(*code),
        Variant::Variant(inner) => color_scheme_code(inner),
        _ => None,
    }
}

fn scheme_is_dark(scheme: ColorScheme) -> bool {
    match scheme {
        ColorScheme::Dark => true,
        ColorScheme::Light => false,
        ColorScheme::NoPreference => gtk_prefers_dark(),
    }
}

fn gtk_prefers_dark() -> bool {
    match gsettings_get("color-scheme").as_deref() {
        Some("prefer-dark") => return true,
        Some("prefer-light") => return false,
        _ => {}
    }
    if gsettings_get("gtk-theme").is_some_and(|theme| theme_name_is_dark(&theme)) {
        return true;
    }
    std::env::var("GTK_THEME").is_ok_and(|theme| theme_name_is_dark(&theme))
}

fn gsettings_get(key: &str) -> Option<String> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().trim_matches('\'').to_string())
}

fn theme_name_is_dark(theme: &str) -> bool {
    theme
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("dark"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray_icon::{self, PlatformStyle, TrayKind};
    use tauri::image::Image;

    #[test]
    fn portal_codes_map_to_scheme() {
        assert_eq!(scheme_from_code(0), ColorScheme::NoPreference);
        assert_eq!(scheme_from_code(1), ColorScheme::Dark);
        assert_eq!(scheme_from_code(2), ColorScheme::Light);
        assert_eq!(scheme_from_code(9), ColorScheme::NoPreference);
    }

    #[test]
    fn nested_variant_unwraps_to_code() {
        let wrapped = Variant::Variant(Box::new(Variant::U32(1)));
        assert_eq!(color_scheme_code(&wrapped), Some(1));
        assert_eq!(color_scheme_code(&Variant::U32(2)), Some(2));
        assert_eq!(color_scheme_code(&Variant::Str("dark".into())), None);
    }

    #[test]
    fn theme_token_dark_matches_gtk_and_cosmic_names() {
        assert!(theme_name_is_dark("adw-gtk3-dark"));
        assert!(theme_name_is_dark("Adwaita:dark"));
        assert!(!theme_name_is_dark("Adwaita"));
        assert!(!theme_name_is_dark("Pop"));
    }

    #[test]
    fn dark_idle_icon_is_white_glyph() {
        let image =
            tray_icon::build(TrayKind::Idle, PlatformStyle::Linux { on_dark: true }).unwrap();
        let rgba = image.rgba();
        assert!(rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|px| px[3] > 0 && px[0] == 255 && px[1] == 255 && px[2] == 255));
        // Still a valid Image so theme watcher can call set_icon.
        let _: &Image<'_> = &image;
    }
}
