//! Update check against the GitHub Releases API. Every platform gets a notification;
//! Windows additionally downloads the signed NSIS installer via `tauri-plugin-updater`
//! and installs it when the user chooses to restart.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Only the NSIS bundle ships signed updater artifacts (see release.yml); `.deb` is not
/// supported by the updater and macOS is not part of the release pipeline.
const AUTO_DOWNLOAD: bool = cfg!(target_os = "windows");

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/janolss/rawlabelprint/releases/latest";
/// Only URLs under this prefix are opened, so a tampered API response cannot redirect elsewhere.
const RELEASES_PAGE_PREFIX: &str = "https://github.com/janolss/rawlabelprint/releases/";
const RELEASES_LATEST_PAGE: &str = "https://github.com/janolss/rawlabelprint/releases/latest";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Well below the unauthenticated limit of 60 requests/hour per IP.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const INITIAL_DELAY: Duration = Duration::from_secs(30);
pub const TRAY_ITEM_ID: &str = "update";
pub const TRAY_ID: &str = "main";

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub url: Option<String>,
    pub available: bool,
    pub checked_at_ms: Option<u64>,
    pub error: Option<String>,
    /// Installer is downloaded and signature-verified; installing restarts the app.
    pub ready_to_install: bool,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
}

struct ReadyUpdate {
    update: Update,
    bytes: Vec<u8>,
}

pub struct UpdateState {
    status: Mutex<UpdateStatus>,
    menu: Menu<Wry>,
    tray_item: Mutex<Option<MenuItem<Wry>>>,
    ready: Mutex<Option<ReadyUpdate>>,
    downloading: AtomicBool,
}

impl UpdateState {
    pub fn new(menu: Menu<Wry>) -> Self {
        Self {
            status: Mutex::new(UpdateStatus {
                current: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            }),
            menu,
            tray_item: Mutex::new(None),
            ready: Mutex::new(None),
            downloading: AtomicBool::new(false),
        }
    }

    pub fn snapshot(&self) -> UpdateStatus {
        self.status.lock().expect("update status poisoned").clone()
    }
}

fn fetch_latest() -> Result<GithubRelease, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into();
    agent
        .get(LATEST_RELEASE_API)
        .header(
            "User-Agent",
            concat!("RawLabelPrint/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|err| format!("Update check failed: {err}"))?
        .body_mut()
        .read_json::<GithubRelease>()
        .map_err(|err| format!("Invalid release response: {err}"))
}

/// `major.minor.patch` with optional leading `v`; pre-release/build suffixes are ignored
/// because `releases/latest` never returns drafts or pre-releases.
fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Runs a blocking check, stores the result, and syncs tray + UI.
pub async fn check_now(app: &AppHandle) -> UpdateStatus {
    let result = tauri::async_runtime::spawn_blocking(fetch_latest)
        .await
        .map_err(|err| format!("Update check task failed: {err}"))
        .and_then(|r| r);

    let state = app.state::<UpdateState>();
    let status = {
        let mut status = state.status.lock().expect("update status poisoned");
        status.checked_at_ms = Some(now_ms());
        match result {
            Ok(release) => {
                let latest = release.tag_name.trim_start_matches('v').to_string();
                status.available = is_newer(&latest, &status.current);
                status.url = release
                    .html_url
                    .starts_with(RELEASES_PAGE_PREFIX)
                    .then_some(release.html_url);
                status.ready_to_install = state
                    .ready
                    .lock()
                    .expect("ready update poisoned")
                    .as_ref()
                    .is_some_and(|r| r.update.version == latest);
                status.latest = Some(latest);
                status.error = None;
            }
            Err(err) => {
                tracing::warn!("{err}");
                status.error = Some(err);
            }
        }
        status.clone()
    };

    if status.available {
        notify(app, &state, &status);
        if AUTO_DOWNLOAD && !status.ready_to_install {
            spawn_download(app.clone());
        }
    } else {
        crate::tray_status::schedule_refresh(app);
    }
    status
}

fn notify(app: &AppHandle, state: &UpdateState, status: &UpdateStatus) {
    if let Err(err) = sync_tray(app, state, status) {
        tracing::error!("Failed to update tray for new version: {err}");
    }
    let _ = app.emit("update-available", status);
    crate::tray_status::schedule_refresh(app);
}

/// Fetches `latest.json` through the updater plugin, downloads and verifies the installer,
/// and keeps the bytes in memory until the user picks "Restart to update".
fn spawn_download(app: AppHandle) {
    let state = app.state::<UpdateState>();
    if state.downloading.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let result = download_update(&app).await;
        let state = app.state::<UpdateState>();
        state.downloading.store(false, Ordering::SeqCst);
        match result {
            Ok(Some(ready)) => {
                let version = ready.update.version.clone();
                *state.ready.lock().expect("ready update poisoned") = Some(ready);
                let status = {
                    let mut status = state.status.lock().expect("update status poisoned");
                    status.ready_to_install = status.latest.as_deref() == Some(version.as_str());
                    status.clone()
                };
                tracing::info!("Update {version} downloaded and ready to install");
                notify(&app, &state, &status);
            }
            Ok(None) => tracing::info!("Updater manifest reports no newer version"),
            Err(err) => tracing::warn!("Update download failed: {err}"),
        }
    });
}

async fn download_update(app: &AppHandle) -> Result<Option<ReadyUpdate>, String> {
    let updater = app.updater().map_err(|err| err.to_string())?;
    let Some(update) = updater.check().await.map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|err| err.to_string())?;
    Ok(Some(ReadyUpdate { update, bytes }))
}

/// Runs the NSIS installer in passive mode; the installer terminates this process itself.
pub fn install_update(app: &AppHandle) -> Result<(), String> {
    let ready = app
        .state::<UpdateState>()
        .ready
        .lock()
        .expect("ready update poisoned")
        .take()
        .ok_or_else(|| "No downloaded update is available".to_string())?;
    ready
        .update
        .install(&ready.bytes)
        .map_err(|err| format!("Update install failed: {err}"))?;
    app.restart()
}

/// Tray entry: install when an installer is ready, otherwise open the release page.
pub fn activate_tray_item(app: &AppHandle) {
    let ready = app.state::<UpdateState>().snapshot().ready_to_install;
    let result = if ready {
        install_update(app)
    } else {
        open_release_page(app)
    };
    if let Err(err) = result {
        tracing::error!("{err}");
    }
}

fn sync_tray(app: &AppHandle, state: &UpdateState, status: &UpdateStatus) -> tauri::Result<()> {
    let latest = status.latest.as_deref().unwrap_or_default();
    let label = if status.ready_to_install {
        format!("Restart to update to v{latest}")
    } else {
        format!("Update available: v{latest}…")
    };
    let mut item = state.tray_item.lock().expect("tray item poisoned");
    match item.as_ref() {
        Some(existing) => existing.set_text(&label)?,
        None => {
            let created = MenuItem::with_id(app, TRAY_ITEM_ID, &label, true, None::<&str>)?;
            // Directly below the app name so it is the first actionable entry.
            state.menu.insert(&created, 1)?;
            *item = Some(created);
        }
    }
    Ok(())
}

pub fn open_release_page(app: &AppHandle) -> Result<(), String> {
    let url = app
        .state::<UpdateState>()
        .snapshot()
        .url
        .unwrap_or_else(|| RELEASES_LATEST_PAGE.to_string());
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| format!("Could not open release page: {err}"))
}

pub fn spawn_periodic_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(INITIAL_DELAY).await;
        let mut interval = tokio::time::interval(CHECK_INTERVAL);
        loop {
            interval.tick().await;
            check_now(&app).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_with_and_without_prefix() {
        assert_eq!(parse_version("v0.4.0"), Some((0, 4, 0)));
        assert_eq!(parse_version("1.12.3"), Some((1, 12, 3)));
        assert_eq!(parse_version("v1.2.3-rc.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("v1.2"), None);
        assert_eq!(parse_version("v1.2.3.4"), None);
        assert_eq!(parse_version("latest"), None);
    }

    #[test]
    fn newer_compares_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.4.0"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.3.9", "0.4.0"));
        assert!(!is_newer("garbage", "0.4.0"));
    }
}
