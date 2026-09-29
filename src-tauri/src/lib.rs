mod config;
mod discovery;
mod http_server;
mod print;
mod state;

use config::{load_config, AppConfig, PrinterInfo};
use discovery::search_zebra_printers;
use print::{get_printer_status, PrinterStatus};
use state::AppState;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

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
    app: AppHandle,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        let restart_needed = cfg.listen_address != listen_address || cfg.port != port;
        cfg.listen_address = listen_address;
        cfg.port = port;
        cfg.launch_at_login = launch_at_login;
        drop(cfg);
        state.persist().await?;
        if restart_needed {
            state.restart_http().await?;
        }
    }

    // Keep autostart plugin in sync
    use tauri_plugin_autostart::ManagerExt;
    let autostart = app.autolaunch();
    if launch_at_login {
        let _ = autostart.enable();
    } else {
        let _ = autostart.disable();
    }

    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn discover_printers(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<PrinterInfo>, String> {
    let printers = tauri::async_runtime::spawn_blocking(search_zebra_printers)
        .await
        .map_err(|e| e.to_string())??;
    *state.discovered.write().await = printers.clone();
    Ok(printers)
}

#[tauri::command]
async fn set_default_printer(
    state: tauri::State<'_, Arc<AppState>>,
    printer: PrinterInfo,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.write().await;
        cfg.default_printer = Some(printer);
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
    if address.trim().is_empty() {
        return Err("Address is required".into());
    }
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
    };
    {
        let mut cfg = state.config.write().await;
        cfg.added_printers
            .retain(|p| p.address != printer.address);
        cfg.added_printers.push(printer.clone());
        cfg.default_printer = Some(printer);
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
        cfg.added_printers.retain(|p| p.address != address);
        if cfg
            .default_printer
            .as_ref()
            .map(|p| p.address == address)
            .unwrap_or(false)
        {
            cfg.default_printer = None;
        }
    }
    state.persist().await?;
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn check_printer_status(printer: PrinterInfo) -> Result<PrinterStatus, String> {
    Ok(tauri::async_runtime::spawn_blocking(move || get_printer_status(&printer))
        .await
        .map_err(|e| e.to_string())?)
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
    tauri::async_runtime::spawn_blocking(move || crate::print::send_raw_to_printer(&target, zpl))
        .await
        .map_err(|e| e.to_string())?
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
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            // Hide from dock on macOS (menu bar / tray agent)
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("RawLabelPrint"));
            std::fs::create_dir_all(&app_data_dir).ok();

            let config = load_config(&app_data_dir);
            let state = Arc::new(AppState::new(app_data_dir, config.clone()));

            // Start HTTP server
            let state_for_http = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = state_for_http.restart_http().await {
                    tracing::error!("HTTP server failed to start: {e}");
                }
            });

            // Sync autostart with saved preference
            {
                use tauri_plugin_autostart::ManagerExt;
                let autostart = app.autolaunch();
                if config.launch_at_login {
                    let _ = autostart.enable();
                }
            }

            app.manage(state);

            let show_i = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("RawLabelPrint")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings(app),
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
                })
                .build(app)?;

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
            remove_added_printer,
            check_printer_status,
            test_print
        ])
        .run(tauri::generate_context!())
        .expect("error while running RawLabelPrint");
}
