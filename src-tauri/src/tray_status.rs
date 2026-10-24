//! Aggregate tray attention/error signals, icon, tooltip, menu rows, and notifications.

use crate::http_server::origin::list_pending;
use crate::print::get_printer_status;
use crate::state::AppState;
use crate::tray_icon::{self, PlatformStyle, TrayKind};
use crate::update_check::{UpdateState, TRAY_ID};
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_notification::{NotificationExt, PermissionState};

pub const PENDING_ITEM_ID: &str = "tray-pending";
pub const OFFLINE_ITEM_ID: &str = "tray-offline";

const PRINTER_POLL_INITIAL: Duration = Duration::from_secs(15);
const PRINTER_POLL_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraySignals {
    pub update_available: bool,
    pub update_latest: Option<String>,
    pub pending_count: usize,
    pub pending_origin: Option<String>,
    pub default_offline: bool,
    pub default_name: Option<String>,
    pub default_detail: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NotifiedFlags {
    update: bool,
    pending: bool,
    offline: bool,
}

pub struct TrayStatusState {
    signals: Mutex<TraySignals>,
    applied_kind: Mutex<TrayKind>,
    notified: Mutex<NotifiedFlags>,
    menu: Menu<Wry>,
    pending_item: Mutex<Option<MenuItem<Wry>>>,
    offline_item: Mutex<Option<MenuItem<Wry>>>,
    #[cfg(target_os = "linux")]
    on_dark: AtomicBool,
}

impl TrayStatusState {
    pub fn new(menu: Menu<Wry>) -> Self {
        Self {
            signals: Mutex::new(TraySignals::default()),
            applied_kind: Mutex::new(TrayKind::Idle),
            notified: Mutex::new(NotifiedFlags::default()),
            menu,
            pending_item: Mutex::new(None),
            offline_item: Mutex::new(None),
            #[cfg(target_os = "linux")]
            on_dark: AtomicBool::new(false),
        }
    }

    #[cfg(target_os = "linux")]
    pub fn set_on_dark(&self, on_dark: bool) {
        self.on_dark.store(on_dark, Ordering::SeqCst);
    }

    #[cfg(target_os = "linux")]
    pub fn on_dark(&self) -> bool {
        self.on_dark.load(Ordering::SeqCst)
    }
}

pub fn resolve(signals: &TraySignals) -> TrayKind {
    if signals.default_offline {
        TrayKind::Error
    } else if signals.update_available || signals.pending_count > 0 {
        TrayKind::Attention
    } else {
        TrayKind::Idle
    }
}

pub fn tooltip(signals: &TraySignals, version: &str) -> String {
    let mut parts = vec![format!("RawLabelPrint {version}")];
    if signals.default_offline {
        let name = signals.default_name.as_deref().unwrap_or("Default printer");
        match signals.default_detail.as_deref() {
            Some(detail) if !detail.is_empty() => {
                parts.push(format!("{name} offline ({detail})"));
            }
            _ => parts.push(format!("{name} offline")),
        }
    }
    if signals.pending_count > 0 {
        let n = signals.pending_count;
        parts.push(if n == 1 {
            "1 website awaiting approval".into()
        } else {
            format!("{n} websites awaiting approval")
        });
    }
    if signals.update_available {
        if let Some(latest) = signals.update_latest.as_deref() {
            parts.push(format!("update {latest} available"));
        } else {
            parts.push("update available".into());
        }
    }
    parts.join(" – ")
}

/// Rising-edge reasons that should trigger a desktop notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyPlan {
    pub update: bool,
    pub pending: bool,
    pub offline: bool,
}

pub(crate) fn plan_notifications(prev: &NotifiedFlags, signals: &TraySignals) -> NotifyPlan {
    NotifyPlan {
        update: signals.update_available && !prev.update,
        pending: signals.pending_count > 0 && !prev.pending,
        offline: signals.default_offline && !prev.offline,
    }
}

fn next_notified(_prev: &NotifiedFlags, signals: &TraySignals) -> NotifiedFlags {
    NotifiedFlags {
        update: signals.update_available,
        pending: signals.pending_count > 0,
        offline: signals.default_offline,
    }
}

/// Re-read live signals (update + pending), keep polled printer fields, apply tray UI.
pub async fn refresh(app: &AppHandle) {
    let Some(signals) = collect_signals(app).await else {
        return;
    };
    apply(app, signals);
}

/// Apply known printer offline state then refresh other signals.
pub fn set_default_printer_status(
    app: &AppHandle,
    offline: bool,
    name: Option<String>,
    detail: Option<String>,
) {
    if let Some(state) = app.try_state::<TrayStatusState>() {
        let mut signals = state.signals.lock().expect("tray signals poisoned");
        signals.default_offline = offline;
        signals.default_name = name;
        signals.default_detail = detail;
    }
    // collect_signals merges update/pending on top of stored printer fields.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh(&app).await;
    });
}

async fn collect_signals(app: &AppHandle) -> Option<TraySignals> {
    let tray_state = app.try_state::<TrayStatusState>()?;
    let mut signals = tray_state
        .signals
        .lock()
        .expect("tray signals poisoned")
        .clone();

    if let Some(update) = app.try_state::<UpdateState>() {
        let snap = update.snapshot();
        signals.update_available = snap.available;
        signals.update_latest = snap.latest;
    }

    if let Some(app_state) = app.try_state::<Arc<AppState>>() {
        let pending = list_pending(&app_state.http_shared()).await;
        signals.pending_count = pending.len();
        signals.pending_origin = pending.first().cloned();
    }

    *tray_state.signals.lock().expect("tray signals poisoned") = signals.clone();
    Some(signals)
}

fn apply(app: &AppHandle, signals: TraySignals) {
    let Some(state) = app.try_state::<TrayStatusState>() else {
        return;
    };
    let kind = resolve(&signals);
    let version = env!("CARGO_PKG_VERSION");
    let tip = tooltip(&signals, version);

    if let Err(err) = apply_icon(app, &state, kind) {
        tracing::warn!("Failed to set tray icon: {err}");
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(err) = tray.set_tooltip(Some(tip)) {
            tracing::warn!("Failed to set tray tooltip: {err}");
        }
    }
    if let Err(err) = sync_menu(app, &state, &signals) {
        tracing::warn!("Failed to sync tray status menu: {err}");
    }

    let plan = {
        let mut notified = state.notified.lock().expect("notified poisoned");
        let plan = plan_notifications(&notified, &signals);
        *notified = next_notified(&notified, &signals);
        plan
    };
    *state.applied_kind.lock().expect("applied kind poisoned") = kind;
    send_notifications(app, &signals, &plan);
}

/// Rebuild icon only (Linux theme changes) using the last applied kind.
pub fn reapply_icon(app: &AppHandle) {
    let Some(state) = app.try_state::<TrayStatusState>() else {
        return;
    };
    let kind = *state.applied_kind.lock().expect("applied kind poisoned");
    if let Err(err) = apply_icon(app, &state, kind) {
        tracing::warn!("Failed to reapply tray icon: {err}");
    }
}

fn apply_icon(app: &AppHandle, state: &TrayStatusState, kind: TrayKind) -> tauri::Result<()> {
    let style = platform_style(state);
    let image = tray_icon::build(kind, style)?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_icon(Some(image))?;
    }
    Ok(())
}

fn platform_style(state: &TrayStatusState) -> PlatformStyle {
    #[cfg(target_os = "macos")]
    {
        let _ = state;
        PlatformStyle::MacTemplate
    }
    #[cfg(target_os = "linux")]
    {
        PlatformStyle::Linux {
            on_dark: state.on_dark(),
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = state;
        PlatformStyle::WindowsColor
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = state;
        PlatformStyle::Linux { on_dark: false }
    }
}

fn sync_menu(app: &AppHandle, state: &TrayStatusState, signals: &TraySignals) -> tauri::Result<()> {
    // Insert below the app name; update_check may already occupy index 1.
    sync_optional_item(
        app,
        &state.menu,
        &state.pending_item,
        PENDING_ITEM_ID,
        signals.pending_count > 0,
        pending_label(signals),
        1,
    )?;
    sync_optional_item(
        app,
        &state.menu,
        &state.offline_item,
        OFFLINE_ITEM_ID,
        signals.default_offline,
        offline_label(signals),
        1,
    )?;
    Ok(())
}

fn pending_label(signals: &TraySignals) -> String {
    match signals.pending_count {
        0 => "Approve website…".into(),
        1 => "Approve website…".into(),
        n => format!("Approve websites ({n})…"),
    }
}

fn offline_label(signals: &TraySignals) -> String {
    match signals.default_name.as_deref() {
        Some(name) if !name.is_empty() => format!("{name} offline…"),
        _ => "Default printer offline…".into(),
    }
}

fn sync_optional_item(
    app: &AppHandle,
    menu: &Menu<Wry>,
    slot: &Mutex<Option<MenuItem<Wry>>>,
    id: &str,
    visible: bool,
    label: String,
    index: usize,
) -> tauri::Result<()> {
    let mut item = slot.lock().expect("tray menu item poisoned");
    if visible {
        match item.as_ref() {
            Some(existing) => existing.set_text(&label)?,
            None => {
                let created = MenuItem::with_id(app, id, &label, true, None::<&str>)?;
                menu.insert(&created, index)?;
                *item = Some(created);
            }
        }
    } else if let Some(existing) = item.take() {
        let _ = menu.remove(&existing);
    }
    Ok(())
}

fn send_notifications(app: &AppHandle, signals: &TraySignals, plan: &NotifyPlan) {
    if !plan.update && !plan.pending && !plan.offline {
        return;
    }
    if let Err(err) = ensure_notification_permission(app) {
        tracing::debug!("Notification permission unavailable: {err}");
        return;
    }
    if plan.offline {
        let name = signals.default_name.as_deref().unwrap_or("Default printer");
        let body = match signals.default_detail.as_deref() {
            Some(detail) if !detail.is_empty() => format!("{name}: {detail}"),
            _ => format!("{name} is unreachable"),
        };
        show_notification(app, "Default printer offline", &body);
    }
    if plan.pending {
        let body = signals
            .pending_origin
            .clone()
            .unwrap_or_else(|| "A website wants to print".into());
        show_notification(app, "Print approval needed", &body);
    }
    if plan.update {
        let body = match signals.update_latest.as_deref() {
            Some(v) => format!("v{v} is ready to download"),
            None => "A newer version is available".into(),
        };
        show_notification(app, "Update available", &body);
    }
}

fn ensure_notification_permission(app: &AppHandle) -> Result<(), String> {
    let api = app.notification();
    let state = api.permission_state().map_err(|e| e.to_string())?;
    if state == PermissionState::Granted {
        return Ok(());
    }
    let state = api.request_permission().map_err(|e| e.to_string())?;
    if state == PermissionState::Granted {
        Ok(())
    } else {
        Err(format!("Notification permission: {state}"))
    }
}

fn show_notification(app: &AppHandle, title: &str, body: &str) {
    if let Err(err) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("Failed to show notification: {err}");
    }
}

/// Background poll of the default printer so the tray can show error without Settings open.
pub fn spawn_default_printer_poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PRINTER_POLL_INITIAL).await;
        let mut interval = tokio::time::interval(PRINTER_POLL_INTERVAL);
        loop {
            interval.tick().await;
            poll_default_printer_once(&app).await;
        }
    });
}

async fn poll_default_printer_once(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let printer = state.config.read().await.default_printer.clone();
    let Some(printer) = printer else {
        set_default_printer_status(app, false, None, None);
        return;
    };
    let name = Some(printer.display_name());
    let permit = match state.device_io.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return,
    };
    let status = match tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        get_printer_status(&printer)
    })
    .await
    {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!("Default printer status task failed: {err}");
            return;
        }
    };
    let offline = status.status == "offline";
    set_default_printer_status(app, offline, name, status.detail);
}

/// Fire-and-forget refresh after approve/deny/config changes.
pub fn schedule_refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh(&app).await;
    });
}

/// Re-probe the default printer (e.g. after the user changes it) then refresh the tray.
pub fn schedule_default_printer_check(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        poll_default_printer_once(&app).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(update: bool, pending: usize, offline: bool) -> TraySignals {
        TraySignals {
            update_available: update,
            update_latest: update.then(|| "1.2.3".into()),
            pending_count: pending,
            pending_origin: (pending > 0).then(|| "https://shop.example".into()),
            default_offline: offline,
            default_name: offline.then(|| "Zebra".into()),
            default_detail: offline.then(|| "unreachable".into()),
        }
    }

    #[test]
    fn resolve_priority_error_over_attention() {
        assert_eq!(resolve(&signals(false, 0, false)), TrayKind::Idle);
        assert_eq!(resolve(&signals(true, 0, false)), TrayKind::Attention);
        assert_eq!(resolve(&signals(false, 2, false)), TrayKind::Attention);
        assert_eq!(resolve(&signals(true, 1, false)), TrayKind::Attention);
        assert_eq!(resolve(&signals(true, 1, true)), TrayKind::Error);
        assert_eq!(resolve(&signals(false, 0, true)), TrayKind::Error);
    }

    #[test]
    fn tooltip_lists_active_reasons() {
        let tip = tooltip(&signals(true, 2, true), "0.5.1");
        assert!(tip.contains("RawLabelPrint 0.5.1"));
        assert!(tip.contains("Zebra offline"));
        assert!(tip.contains("unreachable"));
        assert!(tip.contains("2 websites awaiting approval"));
        assert!(tip.contains("update 1.2.3 available"));
    }

    #[test]
    fn notification_plan_is_rising_edge_only() {
        let idle = NotifiedFlags::default();
        let first = plan_notifications(&idle, &signals(true, 1, true));
        assert_eq!(
            first,
            NotifyPlan {
                update: true,
                pending: true,
                offline: true,
            }
        );
        let after = next_notified(&idle, &signals(true, 1, true));
        let again = plan_notifications(&after, &signals(true, 1, true));
        assert_eq!(
            again,
            NotifyPlan {
                update: false,
                pending: false,
                offline: false,
            }
        );
        let cleared = next_notified(&after, &signals(false, 0, false));
        let reappear = plan_notifications(&cleared, &signals(false, 0, true));
        assert!(reappear.offline);
        assert!(!reappear.update);
        assert!(!reappear.pending);
    }
}
