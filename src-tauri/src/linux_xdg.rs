//! Install a user `.desktop` entry + hicolor icons so Wayland docks (COSMIC) can
//! resolve the window icon. AppImage icons live only inside the mount and are
//! invisible to the compositor's own XDG lookup.
//!
//! COSMIC matches running windows by Wayland `app_id`. For this Tauri/WebKitGTK
//! build that id is the binary name (`rawlabelprint`), not the GTK application id
//! (`se.rawlabelprint.desktop` from `enableGTKAppId`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Wayland app_id reported to COSMIC (binary / StartupWMClass).
const APP_ID: &str = "rawlabelprint";
const ICON_NAME: &str = "rawlabelprint";

const ICON_32: &[u8] = include_bytes!("../icons/32x32.png");
const ICON_64: &[u8] = include_bytes!("../icons/64x64.png");
const ICON_128: &[u8] = include_bytes!("../icons/128x128.png");
const ICON_256: &[u8] = include_bytes!("../icons/128x128@2x.png");
const ICON_512: &[u8] = include_bytes!("../icons/icon.png");

const HICOLOR_INDEX: &[u8] = b"\
[Icon Theme]
Name=Hicolor
Comment=Fallback icon theme
Directories=32x32/apps,64x64/apps,128x128/apps,256x256/apps,512x512/apps

[32x32/apps]
Size=32
Context=Apps
Type=Fixed

[64x64/apps]
Size=64
Context=Apps
Type=Fixed

[128x128/apps]
Size=128
Context=Apps
Type=Fixed

[256x256/apps]
Size=256
Context=Apps
Type=Fixed

[512x512/apps]
Size=512
Context=Apps
Type=Fixed
";

/// Write/update `~/.local/share/{applications,icons}` for dock/launcher matching.
pub fn ensure_desktop_integration() -> io::Result<()> {
    let data_home = xdg_data_home();
    let hicolor = data_home.join("icons/hicolor");
    let mut changed = ensure_hicolor_index(&hicolor)?;
    changed |= install_icons(&hicolor)?;
    changed |= install_desktop_file(&data_home.join("applications"))?;
    // Drop a mistaken GTK-app-id desktop filename if an earlier attempt created it.
    let stale = data_home
        .join("applications")
        .join("se.rawlabelprint.desktop.desktop");
    if fs::remove_file(&stale).is_ok() {
        changed = true;
    }
    if changed {
        refresh_caches(&data_home);
    }
    Ok(())
}

fn xdg_data_home() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    dirs_fallback_home().join(".local/share")
}

fn dirs_fallback_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Create a minimal user hicolor index only when missing; never overwrite an
/// existing theme definition that may list other apps' directories.
fn ensure_hicolor_index(hicolor: &Path) -> io::Result<bool> {
    fs::create_dir_all(hicolor)?;
    let path = hicolor.join("index.theme");
    if path.is_file() {
        return Ok(false);
    }
    // cosmic-launcher (Alt-Tab) resolves Icon= via the Cosmic→hicolor theme chain and
    // needs a valid index.theme under the user hicolor tree.
    fs::write(&path, HICOLOR_INDEX)?;
    Ok(true)
}

fn install_icons(hicolor: &Path) -> io::Result<bool> {
    let mut changed = false;
    for (size, bytes) in [
        ("32x32", ICON_32),
        ("64x64", ICON_64),
        ("128x128", ICON_128),
        ("256x256", ICON_256),
        ("512x512", ICON_512),
    ] {
        let dir = hicolor.join(size).join("apps");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{ICON_NAME}.png"));
        changed |= write_if_changed(&path, bytes)?;
    }
    Ok(changed)
}

fn install_desktop_file(applications: &Path) -> io::Result<bool> {
    fs::create_dir_all(applications)?;
    let path = applications.join(format!("{APP_ID}.desktop"));
    let exec = resolve_exec();
    // Theme name (not absolute path): cosmic-app-list accepts both, but Alt-Tab via
    // cosmic-launcher only looks up names through the Cosmic/hicolor icon theme.
    let contents = format!(
        "\
[Desktop Entry]
Type=Application
Name=RawLabelPrint
Comment=Raw label printing from the browser
Exec={exec}
Icon={ICON_NAME}
Terminal=false
Categories=Utility;
StartupWMClass={APP_ID}
"
    );
    write_if_changed(&path, contents.as_bytes())
}

fn resolve_exec() -> String {
    let raw = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("rawlabelprint"));
    quote_desktop_arg(&raw.to_string_lossy())
}

fn quote_desktop_arg(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".into();
    }
    if value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\\' | '\'' | '`' | '$' | '>' | '<' | '&'))
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

/// Returns `true` when the file was written.
fn write_if_changed(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    if path.is_file()
        && let Ok(existing) = fs::read(path)
        && existing == bytes
    {
        return Ok(false);
    }
    fs::write(path, bytes)?;
    Ok(true)
}

fn refresh_caches(data_home: &Path) {
    let _ = Command::new("update-desktop-database")
        .arg(data_home.join("applications"))
        .status();
    let hicolor = data_home.join("icons/hicolor");
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(&hicolor)
        .status();
    // Alt-Tab (cosmic-launcher → pop-launcher cosmic_toplevel) loads .desktop entries
    // once at plugin start and never refreshes. Restart so the new entry is seen.
    let _ = Command::new("pkill")
        .args(["-x", "cosmic-toplevel"])
        .status();
}

#[cfg(test)]
mod tests {
    use super::quote_desktop_arg;

    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(
            quote_desktop_arg("/home/jan/Applications/Raw Label Print.AppImage"),
            "\"/home/jan/Applications/Raw Label Print.AppImage\""
        );
    }

    #[test]
    fn leaves_simple_paths_alone() {
        assert_eq!(
            quote_desktop_arg("/usr/bin/rawlabelprint"),
            "/usr/bin/rawlabelprint"
        );
    }
}
