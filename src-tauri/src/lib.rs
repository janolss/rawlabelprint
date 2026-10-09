mod config;
mod discovery;
mod http_server;
#[cfg(target_os = "macos")]
mod local_network;
mod print;
mod print_log;
mod state;
mod usb_discovery;

use config::{
    load_config, parse_ip_address, require_listen_port, sanitize_listen_address, AppConfig,
    Connection, PrinterInfo,
};
use http_server::origin::{dismiss_pending, permissions_snapshot, OriginPermissions};
use print::{get_printer_status, PrinterStatus};
use print_log::PrintLogSummary;
use state::AppState;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use usb_discovery::search_all_printers;

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
async fn save_settings(
    state: tauri::State<'_, Arc<AppState>>,
    listen_address: String,
    port: u16,
    launch_at_login: bool,
    browser_print_compatible: bool,
    debug_logging: bool,
    app: AppHandle,
) -> Result<AppConfig, String> {
    {
        let port = require_listen_port(port)?;
        let mut cfg = state.config.write().await;
        let listen_address = sanitize_listen_address(&listen_address);
        let restart_needed = cfg.listen_address != listen_address || cfg.port != port;
        cfg.listen_address = listen_address;
        cfg.port = port;
        cfg.launch_at_login = launch_at_login;
        cfg.browser_print_compatible = browser_print_compatible;
        cfg.debug_logging = debug_logging;
        drop(cfg);
        state.persist().await?;
        if restart_needed {
            state.restart_http().await?;
        }
    }

    // Keep autostart plugin in sync (never register the debug binary).
    use tauri_plugin_autostart::ManagerExt;
    let autostart = app.autolaunch();
    if launch_at_login && autostart_registration_allowed() {
        let _ = autostart.enable();
    } else if !launch_at_login {
        let _ = autostart.disable();
    } else {
        tracing::info!("Skipping autostart enable in debug/dev build");
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
    state: tauri::State<'_, Arc<AppState>>,
    printer: PrinterInfo,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.upsert_printer(printer, true);
    }
    state.persist().await?;
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn add_manual_printer(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    address: String,
    print_port: u16,
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
    };
    {
        let mut cfg = state.config.write().await;
        cfg.upsert_printer(printer, true);
    }
    state.persist().await?;
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn update_printer(
    state: tauri::State<'_, Arc<AppState>>,
    original_address: String,
    name: String,
    address: String,
    print_port: u16,
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
        )?;
    }
    state.persist().await?;
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn remove_added_printer(
    state: tauri::State<'_, Arc<AppState>>,
    address: String,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.remove_printer(&address);
    }
    state.persist().await?;
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn check_printer_status(printer: PrinterInfo) -> Result<PrinterStatus, String> {
    tauri::async_runtime::spawn_blocking(move || get_printer_status(&printer))
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
    Ok(permissions_snapshot(&state.http_shared()).await)
}

#[tauri::command]
async fn deny_origin(
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

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            if let Err(err) = std::fs::create_dir_all(&app_data_dir) {
                tracing::error!(
                    "Could not create config directory {}: {err}",
                    app_data_dir.display()
                );
            }

            let config = load_config(&app_data_dir);
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
                let _ = app.autolaunch().enable();
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
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&app_name_i, &sep, &show_i, &quit_i])?;

            let tray_tooltip = format!("RawLabelPrint {}", env!("CARGO_PKG_VERSION"));
            // Black + alpha only. macOS treats this as a template and tints it.
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            let tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                .tooltip(&tray_tooltip)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "app_name" | "settings" => show_settings(app),
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
            let _tray = tray.build(app)?;

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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_http_status,
            save_settings,
            discover_printers,
            set_default_printer,
            add_manual_printer,
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
            revoke_origin
        ])
        .run(tauri::generate_context!())
        .expect("error while running RawLabelPrint");
}
