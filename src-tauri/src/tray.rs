// System tray (menu bar) — the primary interface for GhostBloods

use tauri::{
    menu::{Menu, MenuItem, Submenu},
    tray::TrayIconBuilder,
    Manager,
};

pub fn create_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let open_dashboard = MenuItem::with_id(app, "open_dashboard", "🌐  Open Dashboard", true, None::<&str>)?;
    let force_check = MenuItem::with_id(app, "force_check", "🔍  Force Background Check", true, None::<&str>)?;

    let mute_1h = MenuItem::with_id(app, "mute_1h", "1 Hour", true, None::<&str>)?;
    let mute_8h = MenuItem::with_id(app, "mute_8h", "8 Hours", true, None::<&str>)?;
    let mute_24h = MenuItem::with_id(app, "mute_24h", "24 Hours", true, None::<&str>)?;
    let unmute = MenuItem::with_id(app, "unmute", "🔔  Unmute", true, None::<&str>)?;

    let mute_submenu = Submenu::with_items(
        app,
        "🔇  Mute Notifications",
        true,
        &[&mute_1h, &mute_8h, &mute_24h, &unmute],
    )?;

    let settings = MenuItem::with_id(app, "settings", "⚙️  Settings", true, None::<&str>)?;
    let separator = MenuItem::with_id(app, "sep", "───────────", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "✖  Quit GhostBloods", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_dashboard,
            &force_check,
            &mute_submenu,
            &settings,
            &separator,
            &quit,
        ],
    )?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("GhostBloods — Intelligence Monitor")
        .on_menu_event(move |app, event| {
            match event.id().as_ref() {
                "open_dashboard" => {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        match crate::cmd_open_dashboard(handle).await {
                            Ok(_) => println!("[GhostBloods] Dashboard opened"),
                            Err(e) => eprintln!("[GhostBloods] Failed to open dashboard: {}", e),
                        }
                    });
                }
                "force_check" => {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        match crate::cmd_force_check(handle).await {
                            Ok(msg) => println!("[GhostBloods] Force check: {}", msg),
                            Err(e) => eprintln!("[GhostBloods] Force check failed: {}", e),
                        }
                    });
                }
                "mute_1h" => {
                    crate::notifier::mute_for(app, 1.0);
                    println!("[GhostBloods] Muted for 1 hour");
                }
                "mute_8h" => {
                    crate::notifier::mute_for(app, 8.0);
                    println!("[GhostBloods] Muted for 8 hours");
                }
                "mute_24h" => {
                    crate::notifier::mute_for(app, 24.0);
                    println!("[GhostBloods] Muted for 24 hours");
                }
                "unmute" => {
                    crate::notifier::unmute(app);
                    println!("[GhostBloods] Unmuted");
                }
                "settings" => {
                    open_settings_window(app);
                }
                "quit" => {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::engine::stop_engine(&handle).await;
                        handle.exit(0);
                    });
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

fn open_settings_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _ = tauri::WebviewWindowBuilder::new(
            app,
            "settings",
            tauri::WebviewUrl::App("settings.html".into()),
        )
        .title("GhostBloods — Settings")
        .inner_size(520.0, 640.0)
        .resizable(false)
        .center()
        .build();
    }
}
