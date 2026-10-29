mod config;
mod discovery;
mod http_server;
#[cfg(target_os = "linux")]
mod linux_tray;
#[cfg(target_os = "macos")]
mod local_network;
mod print;
mod print_log;
mod state;
mod tray_icon;
mod tray_status;
mod update_check;
mod usb_discovery;

use config::{
    import_printer_site, load_config, parse_ip_address, require_listen_port,
    sanitize_listen_address, save_config, AppConfig, Connection, PrinterInfo, ThemePreference,
};
use http_server::origin::{dismiss_pending, permissions_snapshot, OriginPermissions};
use print::{get_printer_status, PrinterStatus};
use print_log::PrintLogSummary;
use state::AppState;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::path::BaseDirectory;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Theme, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_opener::OpenerExt;
use tray_icon::{PlatformStyle, TrayKind};
use tray_status::TrayStatusState;
use update_check::{UpdateState, UpdateStatus};
use usb_discovery::search_all_printers;

/// Rendered HTML (from `docs/getting-started.md` via `npm run docs:getting-started`).
const GETTING_STARTED_REL: &str = "docs/getting-started.html";

/// LaunchAgent must not point at `target/debug` (breaks Local Network / identity).
fn autostart_registration_allowed() -> bool {
    !cfg!(debug_assertions)
}

#[tauri::command]
async fn get_config(state: tauri::State<'_, Arc<AppState>>) -> Result<AppConfig, String> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn get_http_status(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state.http_status.lock().await.clone())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri injects State/AppHandle; settings stay flat for invoke()
async fn save_settings(
    state: tauri::State<'_, Arc<AppState>>,
    listen_address: String,
    port: u16,
    launch_at_login: bool,
    browser_print_compatible: bool,
    debug_logging: bool,
    allow_local_scripts: bool,
    theme: ThemePreference,
    app: AppHandle,
) -> Result<AppConfig, String> {
    let port = require_listen_port(port)?;
    let listen_address = sanitize_listen_address(&listen_address);
    use tauri_plugin_autostart::ManagerExt;
    let autostart = app.autolaunch();
    let mut cfg = state.config.write().await;
    let previous = cfg.clone();
    let restart_needed = previous.listen_address != listen_address || previous.port != port;
    let theme_changed = theme != previous.theme;
    let autostart_changed = launch_at_login != previous.launch_at_login
        && (!launch_at_login || autostart_registration_allowed());

    if autostart_changed {
        let result = if launch_at_login {
            autostart.enable()
        } else {
            autostart.disable()
        };
        result.map_err(|err| format!("Failed to update autostart setting: {err}"))?;
    } else if launch_at_login && !autostart_registration_allowed() {
        tracing::info!("Skipping autostart enable in debug/dev build");
    }

    let mut updated = previous.clone();
    updated.listen_address = listen_address;
    updated.port = port;
    updated.launch_at_login = launch_at_login;
    updated.browser_print_compatible = browser_print_compatible;
    updated.debug_logging = debug_logging;
    updated.allow_local_scripts = allow_local_scripts;
    updated.theme = theme;
    if let Err(err) = save_config(&state.app_data_dir, &updated) {
        let rollback_result = if autostart_changed {
            if previous.launch_at_login {
                autostart.enable()
            } else {
                autostart.disable()
            }
        } else {
            Ok(())
        };
        return match rollback_result {
            Ok(()) => Err(err),
            Err(rollback_err) => Err(format!(
                "{err}; failed to restore autostart setting: {rollback_err}"
            )),
        };
    }
    *cfg = updated;
    drop(cfg);
    if theme_changed {
        apply_settings_window_theme(&app, theme);
    }
    if restart_needed {
        state.restart_http().await?;
    }

    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn discover_printers() -> Result<Vec<PrinterInfo>, String> {
    let printers = tauri::async_runtime::spawn_blocking(search_all_printers)
        .await
        .map_err(|e| e.to_string())??;
    Ok(printers)
}

#[tauri::command]
async fn set_default_printer(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    printer: PrinterInfo,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.upsert_printer(printer, true);
    }
    state.persist().await?;
    tray_status::schedule_default_printer_check(&app);
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn save_printer(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    printer: PrinterInfo,
    make_default: bool,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.upsert_printer(printer, make_default);
    }
    state.persist().await?;
    tray_status::schedule_default_printer_check(&app);
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn add_manual_printer(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    address: String,
    print_port: u16,
    make_default: bool,
) -> Result<AppConfig, String> {
    let address = parse_ip_address(&address)?;
    let printer = PrinterInfo {
        name: Some(if name.trim().is_empty() {
            address.clone()
        } else {
            name
        }),
        model: "Manual".into(),
        firmware: String::new(),
        serial_number: String::new(),
        address,
        port: 0,
        print_port: if print_port == 0 { 9100 } else { print_port },
        config_port: 80,
        connection: Connection::Network,
        capture_path: None,
    };
    {
        let mut cfg = state.config.write().await;
        cfg.upsert_printer(printer, make_default);
    }
    state.persist().await?;
    tray_status::schedule_default_printer_check(&app);
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn add_virtual_printer(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    capture_path: Option<String>,
    make_default: bool,
) -> Result<AppConfig, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name is required".into());
    }
    let capture_path = capture_path.and_then(|path| {
        let trimmed = path.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let printer = PrinterInfo {
        name: Some(name.to_string()),
        model: String::new(),
        firmware: String::new(),
        serial_number: String::new(),
        address: name.to_string(),
        port: 0,
        print_port: 0,
        config_port: 0,
        connection: Connection::Virtual,
        capture_path,
    };
    {
        let mut cfg = state.config.write().await;
        cfg.upsert_printer(printer, make_default);
    }
    state.persist().await?;
    tray_status::schedule_default_printer_check(&app);
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn update_printer(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    original_address: String,
    name: String,
    address: String,
    print_port: u16,
    capture_path: Option<String>,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.update_printer(
            &original_address,
            if name.trim().is_empty() {
                None
            } else {
                Some(name)
            },
            address,
            print_port,
            capture_path,
        )?;
    }
    state.persist().await?;
    tray_status::schedule_default_printer_check(&app);
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn import_printers_and_websites(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    payload: String,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        let mut next = cfg.clone();
        import_printer_site(&mut next, &payload)?;
        save_config(&state.app_data_dir, &next)?;
        *cfg = next;
    }
    tray_status::schedule_default_printer_check(&app);
    tray_status::schedule_refresh(&app);
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn remove_added_printer(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    address: String,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.remove_printer(&address);
    }
    state.persist().await?;
    tray_status::schedule_default_printer_check(&app);
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn check_printer_status(
    state: tauri::State<'_, Arc<AppState>>,
    printer: PrinterInfo,
) -> Result<PrinterStatus, String> {
    let permit = state
        .device_io
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Printer status check limit reached; retry shortly".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        get_printer_status(&printer)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn test_print(
    state: tauri::State<'_, Arc<AppState>>,
    printer: Option<PrinterInfo>,
) -> Result<(), String> {
    let target = match printer {
        Some(p) => p,
        None => state
            .config
            .read()
            .await
            .default_printer
            .clone()
            .ok_or_else(|| "No default printer selected".to_string())?,
    };
    // Minimal ZPL test label
    let zpl = "^XA^FO50,50^A0N,40,40^FDRawLabelPrint OK^FS^XZ";
    let logged = target.clone();
    let zpl_for_send = zpl.to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::print::send_raw_to_printer(&target, &zpl_for_send)
    })
    .await
    .map_err(|e| e.to_string())?;
    state
        .record_print_log("test_print", &logged, zpl.as_bytes(), result.clone())
        .await;
    result
}

#[tauri::command]
async fn get_print_log(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<PrintLogSummary>, String> {
    Ok(state.print_log.list_summaries())
}

#[tauri::command]
async fn clear_print_log(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state.print_log.clear();
    Ok(())
}

#[tauri::command]
async fn resend_print_log(state: tauri::State<'_, Arc<AppState>>, id: u64) -> Result<(), String> {
    state.resend_print_log(id).await
}

#[tauri::command]
async fn get_origin_permissions(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<OriginPermissions, String> {
    Ok(permissions_snapshot(&state.http_shared()).await)
}

#[tauri::command]
async fn approve_origin(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    origin: String,
) -> Result<OriginPermissions, String> {
    let key = AppConfig::normalize_origin(&origin);
    if key.is_empty() {
        return Err("Origin is required".into());
    }
    {
        let mut cfg = state.config.write().await;
        cfg.allow_origin(&key);
    }
    dismiss_pending(&state.http_shared(), &key).await;
    state.persist().await?;
    tray_status::schedule_refresh(&app);
    Ok(permissions_snapshot(&state.http_shared()).await)
}

#[tauri::command]
async fn deny_origin(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    origin: String,
) -> Result<OriginPermissions, String> {
    let key = AppConfig::normalize_origin(&origin);
    if key.is_empty() {
        return Err("Origin is required".into());
    }
    {
        let mut cfg = state.config.write().await;
        cfg.deny_origin(&key);
    }
    dismiss_pending(&state.http_shared(), &key).await;
    state.persist().await?;
    tray_status::schedule_refresh(&app);
    Ok(permissions_snapshot(&state.http_shared()).await)
}

#[tauri::command]
async fn remove_denied_origin(
    state: tauri::State<'_, Arc<AppState>>,
    origin: String,
) -> Result<OriginPermissions, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.remove_denied_origin(&origin);
    }
    state.persist().await?;
    Ok(permissions_snapshot(&state.http_shared()).await)
}

#[tauri::command]
async fn revoke_origin(
    state: tauri::State<'_, Arc<AppState>>,
    origin: String,
) -> Result<OriginPermissions, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.revoke_origin(&origin);
    }
    state.persist().await?;
    Ok(permissions_snapshot(&state.http_shared()).await)
}

#[tauri::command]
fn get_update_status(state: tauri::State<'_, UpdateState>) -> UpdateStatus {
    state.snapshot()
}

#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<UpdateStatus, String> {
    Ok(update_check::check_now(&app).await)
}

#[tauri::command]
fn open_release_page(app: AppHandle) -> Result<(), String> {
    update_check::open_release_page(&app)
}

#[tauri::command]
fn install_update(app: AppHandle) -> Result<(), String> {
    update_check::install_update(&app)
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Sync native titlebar/chrome with the Settings theme preference.
fn apply_settings_window_theme(app: &AppHandle, preference: ThemePreference) {
    let Some(window) = app.get_webview_window("settings") else {
        return;
    };
    let theme = match preference {
        ThemePreference::System => None,
        ThemePreference::Light => Some(Theme::Light),
        ThemePreference::Dark => Some(Theme::Dark),
    };
    if let Err(err) = window.set_theme(theme) {
        tracing::warn!("Failed to set settings window theme: {err}");
    }
}

/// Bundled resource in release; repo file next to `src-tauri` in `tauri dev`.
fn getting_started_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(bundled) = app
        .path()
        .resolve(GETTING_STARTED_REL, BaseDirectory::Resource)
    {
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(GETTING_STARTED_REL);
    let dev = dev
        .canonicalize()
        .map_err(|err| format!("Getting Started guide not found: {err}"))?;
    if dev.is_file() {
        return Ok(dev);
    }
    Err(format!(
        "Getting Started guide not found at {}",
        dev.display()
    ))
}

fn open_getting_started(app: &AppHandle) {
    match getting_started_path(app) {
        Ok(path) => {
            if let Err(err) = app.opener().open_path(path.to_string_lossy(), None::<&str>) {
                tracing::error!("Failed to open Getting Started guide: {err}");
            }
        }
        Err(err) => tracing::error!("{err}"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Single-instance must be registered first so a second launch never boots HTTP/tray.
    let mut builder = tauri::Builder::default();
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            // Hide from dock on macOS (menu bar / tray agent).
            // Briefly use Regular first so the Local Network permission dialog can appear.
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Regular);
                std::thread::spawn(|| {
                    local_network::trigger_local_network_permission_prompt();
                });
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(4));
                    let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                });
            }

            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("RawLabelPrint"));
            std::fs::create_dir_all(&app_data_dir).map_err(|err| {
                format!(
                    "Could not create config directory {}: {err}",
                    app_data_dir.display()
                )
            })?;

            let config = load_config(&app_data_dir)
                .map_err(|err| format!("Failed to load application configuration: {err}"))?;
            let state = Arc::new(AppState::new(app_data_dir, config.clone()));

            let weak_state = Arc::downgrade(&state);
            tauri::async_runtime::spawn(async move {
                let mut cleanup = tokio::time::interval(std::time::Duration::from_secs(15));
                loop {
                    cleanup.tick().await;
                    let Some(state) = weak_state.upgrade() else {
                        break;
                    };
                    state.sessions.purge_idle_sessions();
                }
            });

            // Open Settings when a new website needs print approval.
            let mut pending_rx = state.subscribe_pending_origins();
            let app_for_pending = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match pending_rx.recv().await {
                        Ok(origin) => {
                            tracing::info!("Print approval needed for origin {origin}");
                            let _ = app_for_pending.emit("origin-pending", &origin);
                            show_settings(&app_for_pending);
                            tray_status::schedule_refresh(&app_for_pending);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            // Start HTTP server
            let state_for_http = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = state_for_http.restart_http().await {
                    tracing::error!("HTTP server failed to start: {e}");
                }
            });

            // Sync autostart with saved preference (release builds only).
            if config.launch_at_login && autostart_registration_allowed() {
                use tauri_plugin_autostart::ManagerExt;
                if let Err(err) = app.autolaunch().enable() {
                    tracing::error!("Failed to enable autostart: {err}");
                }
            } else if config.launch_at_login {
                tracing::info!("Skipping autostart sync in debug/dev build");
            }

            app.manage(state);

            // Keep enabled so the label uses normal menu text color (disabled items are
            // nearly invisible on macOS dark menu bar menus).
            let app_name_i =
                MenuItem::with_id(app, "app_name", "RawLabelPrint", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let show_i = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let help_i = MenuItem::with_id(
                app,
                "getting_started",
                "Getting Started…",
                true,
                None::<&str>,
            )?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&app_name_i, &sep, &show_i, &help_i, &quit_i])?;

            let tray_tooltip = format!("RawLabelPrint {}", env!("CARGO_PKG_VERSION"));
            let tray_status = TrayStatusState::new(menu.clone());
            #[cfg(target_os = "linux")]
            {
                let on_dark = linux_tray::prefers_dark_ui();
                tray_status.set_on_dark(on_dark);
            }
            let initial_style = {
                #[cfg(target_os = "macos")]
                {
                    PlatformStyle::MacTemplate
                }
                #[cfg(target_os = "linux")]
                {
                    PlatformStyle::Linux {
                        on_dark: tray_status.on_dark(),
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    PlatformStyle::WindowsColor
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
                {
                    PlatformStyle::Linux { on_dark: false }
                }
            };
            let tray_icon_image = tray_icon::build(TrayKind::Idle, initial_style)?;
            app.manage(UpdateState::new(menu.clone()));
            app.manage(tray_status);
            let tray = TrayIconBuilder::with_id(update_check::TRAY_ID)
                .icon(tray_icon_image)
                .menu(&menu)
                .tooltip(&tray_tooltip)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "app_name" | "settings" => show_settings(app),
                    "getting_started" => open_getting_started(app),
                    update_check::TRAY_ITEM_ID => update_check::activate_tray_item(app),
                    tray_status::PENDING_ITEM_ID | tray_status::OFFLINE_ITEM_ID => {
                        show_settings(app)
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_settings(tray.app_handle());
                    }
                });
            // Template icons are a macOS menu-bar convention; on Linux they often render blank/grey.
            #[cfg(target_os = "macos")]
            let tray = tray.icon_as_template(true);
            let _built = tray.build(app)?;
            #[cfg(target_os = "linux")]
            linux_tray::follow_color_scheme(app.handle().clone());

            tray_status::spawn_default_printer_poll(app.handle().clone());

            // Dev builds report an arbitrary version; manual "Check for updates" still works.
            if !cfg!(debug_assertions) {
                update_check::spawn_periodic_check(app.handle().clone());
            }

            // Close settings to tray instead of quitting
            if let Some(window) = app.get_webview_window("settings") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            apply_settings_window_theme(app.handle(), config.theme);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_http_status,
            save_settings,
            discover_printers,
            set_default_printer,
            save_printer,
            add_manual_printer,
            add_virtual_printer,
            import_printers_and_websites,
            update_printer,
            remove_added_printer,
            check_printer_status,
            test_print,
            get_print_log,
            clear_print_log,
            resend_print_log,
            get_origin_permissions,
            approve_origin,
            deny_origin,
            remove_denied_origin,
            revoke_origin,
            get_update_status,
            check_for_updates,
            open_release_page,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running RawLabelPrint");
}

#[cfg(test)]
mod getting_started_tests {
    use super::GETTING_STARTED_REL;
    use std::path::Path;

    #[test]
    fn guide_exists_in_repo() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(GETTING_STARTED_REL);
        assert!(
            path.is_file(),
            "expected {} (run npm run docs:getting-started)",
            path.display()
        );
        let md = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("docs/getting-started.md");
        assert!(md.is_file(), "markdown source missing: {}", md.display());
    }
}
