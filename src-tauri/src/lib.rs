// GhostBloods — Tauri v2 Desktop App
// Lightweight intelligence dashboard with menu-bar tray + two modes

mod engine;
mod tray;
mod notifier;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Initialize the system tray
            tray::create_tray(app.handle())?;

            // Start the background notifier timer
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                notifier::start_background_loop(handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_open_dashboard,
            cmd_close_dashboard,
            cmd_force_check,
            cmd_mute,
            cmd_get_settings,
            cmd_save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running GhostBloods");
}

// === Tauri Commands ===

#[tauri::command]
async fn cmd_open_dashboard(app: tauri::AppHandle) -> Result<String, String> {
    // Start the Node engine
    engine::start_engine(&app).await.map_err(|e| e.to_string())?;

    // Create/show the dashboard window
    if let Some(window) = app.get_webview_window("dashboard") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    } else {
        let _window = tauri::WebviewWindowBuilder::new(
            &app,
            "dashboard",
            tauri::WebviewUrl::External("http://localhost:3117".parse().unwrap()),
        )
        .title("GhostBloods — Intelligence Terminal")
        .inner_size(1400.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .resizable(true)
        .center()
        .build()
        .map_err(|e| e.to_string())?;

        // When dashboard window is closed, stop the engine (notifier mode)
        let handle = app.clone();
        let win = app.get_webview_window("dashboard").unwrap();
        win.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Some(w) = handle.get_webview_window("dashboard") {
                    let _ = w.hide();
                }
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = engine::stop_engine(&h).await;
                });
            }
        });
    }

    Ok("Dashboard opened".to_string())
}

#[tauri::command]
async fn cmd_close_dashboard(app: tauri::AppHandle) -> Result<String, String> {
    if let Some(window) = app.get_webview_window("dashboard") {
        let _ = window.hide();
    }
    engine::stop_engine(&app).await.map_err(|e| e.to_string())?;
    Ok("Dashboard closed".to_string())
}

#[tauri::command]
async fn cmd_force_check(app: tauri::AppHandle) -> Result<String, String> {
    notifier::run_lite_sweep(&app).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_mute(app: tauri::AppHandle, hours: f64) -> Result<String, String> {
    notifier::mute_for(&app, hours);
    Ok(format!("Muted for {} hours", hours))
}

#[tauri::command]
async fn cmd_get_settings(app: tauri::AppHandle) -> Result<String, String> {
    let state = notifier::load_state(&app);
    serde_json::to_string(&state).map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_save_settings(app: tauri::AppHandle, settings_json: String) -> Result<String, String> {
    notifier::save_settings(&app, &settings_json).map_err(|e| e.to_string())?;
    Ok("Settings saved".to_string())
}
