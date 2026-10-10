//! Tray glyph color on Linux.
//!
//! The bundled icon is a black template. macOS tints templates in the menu bar.
//! StatusNotifier hosts (COSMIC, GNOME AppIndicator, KDE) draw the pixmap unchanged,
//! so the same shape is painted white when the desktop prefers a dark appearance.

use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::Runtime;
use zbus::zvariant::{OwnedValue, Value};

const APPEARANCE_NS: &str = "org.freedesktop.appearance";
const COLOR_SCHEME_KEY: &str = "color-scheme";
const TRAY_PNG: &[u8] = include_bytes!("../icons/tray.png");

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
    fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;

    #[zbus(signal)]
    fn setting_changed(&self, namespace: &str, key: &str, value: OwnedValue) -> zbus::Result<()>;
}

pub fn icon(on_dark: bool) -> tauri::Result<Image<'static>> {
    let image = Image::from_bytes(TRAY_PNG)?;
    if !on_dark {
        return Ok(image);
    }
    let mut rgba = image.rgba().to_vec();
    paint_template_white(&mut rgba);
    Ok(Image::new_owned(rgba, image.width(), image.height()))
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
pub fn follow_color_scheme<R: Runtime>(tray: TrayIcon<R>) {
    if let Err(err) = std::thread::Builder::new()
        .name("tray-theme".into())
        .spawn(move || {
            if let Err(err) = watch(tray) {
                tracing::warn!("Tray color-scheme watcher stopped: {err}");
            }
        })
    {
        tracing::warn!("Failed to start tray color-scheme watcher: {err}");
    }
}

fn watch<R: Runtime>(tray: TrayIcon<R>) -> zbus::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = PortalSettingsProxyBlocking::new(&connection)?;
    let mut dark = scheme_is_dark(read_scheme(&proxy)?);
    for signal in proxy.receive_setting_changed()? {
        let args = signal.args()?;
        if *args.namespace() != APPEARANCE_NS || *args.key() != COLOR_SCHEME_KEY {
            continue;
        }
        let next = scheme_is_dark(scheme_from_value(args.value()));
        if next == dark {
            continue;
        }
        dark = next;
        match icon(dark) {
            Ok(image) => {
                if let Err(err) = tray.set_icon(Some(image)) {
                    tracing::warn!("Failed to update tray icon: {err}");
                }
            }
            Err(err) => tracing::warn!("Failed to build tray icon: {err}"),
        }
    }
    Ok(())
}

fn read_portal_scheme() -> zbus::Result<ColorScheme> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = PortalSettingsProxyBlocking::new(&connection)?;
    read_scheme(&proxy)
}

fn read_scheme(proxy: &PortalSettingsProxyBlocking<'_>) -> zbus::Result<ColorScheme> {
    let value = proxy.read_one(APPEARANCE_NS, COLOR_SCHEME_KEY)?;
    Ok(scheme_from_value(&value))
}

fn scheme_from_value(value: &Value<'_>) -> ColorScheme {
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

fn color_scheme_code(value: &Value<'_>) -> Option<u32> {
    match value {
        Value::U32(code) => Some(*code),
        Value::Value(inner) => color_scheme_code(inner),
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

fn paint_template_white(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        if px[3] != 0 {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_codes_map_to_scheme() {
        assert_eq!(scheme_from_code(0), ColorScheme::NoPreference);
        assert_eq!(scheme_from_code(1), ColorScheme::Dark);
        assert_eq!(scheme_from_code(2), ColorScheme::Light);
        assert_eq!(scheme_from_code(9), ColorScheme::NoPreference);
    }

    #[test]
    fn nested_variant_unwraps_to_code() {
        let wrapped = Value::Value(Box::new(Value::U32(1)));
        assert_eq!(color_scheme_code(&wrapped), Some(1));
        assert_eq!(color_scheme_code(&Value::U32(2)), Some(2));
        assert_eq!(color_scheme_code(&Value::Str("dark".into())), None);
    }

    #[test]
    fn theme_token_dark_matches_gtk_and_cosmic_names() {
        assert!(theme_name_is_dark("adw-gtk3-dark"));
        assert!(theme_name_is_dark("Adwaita:dark"));
        assert!(!theme_name_is_dark("Adwaita"));
        assert!(!theme_name_is_dark("Pop"));
    }

    #[test]
    fn white_template_keeps_alpha() {
        let source = Image::from_bytes(TRAY_PNG).unwrap();
        let mut rgba = source.rgba().to_vec();
        assert!(rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|px| px[3] > 0 && px[0] < 32));
        paint_template_white(&mut rgba);
        for px in rgba.as_chunks::<4>().0 {
            if px[3] != 0 {
                assert_eq!(&px[..3], &[255, 255, 255]);
            }
        }
        assert!(rgba.as_chunks::<4>().0.iter().any(|px| px[3] == 0));
    }
}
