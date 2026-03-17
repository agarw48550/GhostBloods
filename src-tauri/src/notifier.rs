// Background notifier — lightweight sweep loop + native macOS notifications

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

static MUTE_UNTIL: Mutex<Option<u64>> = Mutex::new(None);
static ALERT_TIMESTAMPS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifierState {
    pub last_sweep: Option<String>,
    pub alert_hashes: Vec<String>,
    pub settings: NotifierSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifierSettings {
    pub interval_minutes: u64,
    pub threshold: u8,
    pub max_notifications_per_hour: u8,
    pub digest_mode: bool,
    pub quiet_hours_start: Option<u8>,
    pub quiet_hours_end: Option<u8>,
    pub watchlist_keywords: Vec<String>,
    pub watchlist_regions: Vec<String>,
}

impl Default for NotifierSettings {
    fn default() -> Self {
        Self {
            interval_minutes: 45,
            threshold: 8,
            max_notifications_per_hour: 3,
            digest_mode: true,
            quiet_hours_start: None,
            quiet_hours_end: None,
            watchlist_keywords: vec![
                "nuclear".to_string(),
                "missile".to_string(),
                "invasion".to_string(),
                "sanctions".to_string(),
            ],
            watchlist_regions: vec![
                "Ukraine".to_string(),
                "Taiwan".to_string(),
                "Middle East".to_string(),
                "South China Sea".to_string(),
            ],
        }
    }
}

impl Default for NotifierState {
    fn default() -> Self {
        Self {
            last_sweep: None,
            alert_hashes: Vec::new(),
            settings: NotifierSettings::default(),
        }
    }
}

/// Main background loop — runs the lite sweep on a timer
pub async fn start_background_loop(app: tauri::AppHandle) {
    println!("[GhostBloods] Background notifier started");

    // Initial delay: wait 30 seconds before first check
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    loop {
        let interval = {
            let state = load_state(&app);
            state.settings.interval_minutes
        };

        // Run the lite sweep (only if engine is NOT running)
        if !crate::engine::is_engine_running() {
            match run_lite_sweep(&app).await {
                Ok(msg) => println!("[GhostBloods] Lite sweep: {}", msg),
                Err(e) => eprintln!("[GhostBloods] Lite sweep error: {}", e),
            }
        } else {
            println!("[GhostBloods] Skipping lite sweep — engine is running (dashboard mode)");
        }

        // Sleep for the configured interval
        tokio::time::sleep(std::time::Duration::from_secs(interval * 60)).await;
    }
}

/// Run a single lite sweep via the Node.js notifier script
pub async fn run_lite_sweep(app: &tauri::AppHandle) -> Result<String, Box<dyn std::error::Error>> {
    if is_muted() {
        return Ok("Muted — skipping".to_string());
    }

    let project_root = find_project_root();
    let sweep_script = project_root.join("notifier").join("lite-sweep.mjs");

    if !sweep_script.exists() {
        return Err(format!("Sweep script not found: {:?}", sweep_script).into());
    }

    // Load state for the sweep
    let state = load_state(app);
    let state_json = serde_json::to_string(&state)?;

    println!("[GhostBloods] Running lite sweep...");

    let output = tokio::process::Command::new("node")
        .arg(&sweep_script)
        .env("GHOSTBLOODS_STATE", &state_json)
        .current_dir(&project_root)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Lite sweep failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse the JSON output
    if let Ok(result) = serde_json::from_str::<SweepResult>(&stdout) {
        let alert_count = result.alerts.len();

        if !result.alerts.is_empty() {
            send_notifications(app, &result.alerts, &state.settings);

            // Update state with new hashes
            let mut new_state = state;
            new_state.last_sweep = Some(chrono_now());
            for alert in &result.alerts {
                if let Some(hash) = &alert.hash {
                    new_state.alert_hashes.push(hash.clone());
                }
            }
            // Keep only last 500 hashes
            if new_state.alert_hashes.len() > 500 {
                let len = new_state.alert_hashes.len();
                new_state.alert_hashes = new_state.alert_hashes[len - 500..].to_vec();
            }
            save_state(app, &new_state);
        }

        Ok(format!("{} alerts from {} items", alert_count, result.stats.total_items))
    } else {
        Ok(format!("Sweep completed (raw output: {} bytes)", stdout.len()))
    }
}

#[derive(Debug, Deserialize)]
struct SweepResult {
    alerts: Vec<SweepAlert>,
    stats: SweepStats,
}

#[derive(Debug, Deserialize)]
struct SweepAlert {
    title: String,
    tier: String,
    score: f64,
    source: String,
    hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SweepStats {
    total_items: u32,
    #[allow(dead_code)]
    sources_checked: u32,
    #[allow(dead_code)]
    duration_ms: u64,
}

fn send_notifications(app: &tauri::AppHandle, alerts: &[SweepAlert], settings: &NotifierSettings) {
    if is_muted() {
        return;
    }

    // Rate limiting: check how many notifications sent in last hour
    let now = now_epoch();
    let hour_ago = now.saturating_sub(3600);
    let recent_count = {
        let timestamps = ALERT_TIMESTAMPS.lock().unwrap();
        timestamps.iter().filter(|&&t| t > hour_ago).count()
    };

    if recent_count >= settings.max_notifications_per_hour as usize {
        println!("[GhostBloods] Rate limited — {} notifications in last hour", recent_count);
        return;
    }

    // Check quiet hours
    if is_quiet_hours(settings) {
        // Only allow FLASH during quiet hours
        let flash_alerts: Vec<&SweepAlert> = alerts.iter().filter(|a| a.tier == "FLASH").collect();
        if flash_alerts.is_empty() {
            println!("[GhostBloods] Quiet hours — suppressing non-FLASH alerts");
            return;
        }
        // Send only FLASH alerts
        for alert in flash_alerts {
            send_single_notification(app, alert);
        }
        return;
    }

    // Digest mode: if more than 3 alerts, send one summary
    if settings.digest_mode && alerts.len() > 3 {
        let flash_count = alerts.iter().filter(|a| a.tier == "FLASH").count();
        let priority_count = alerts.iter().filter(|a| a.tier == "PRIORITY").count();

        let title = if flash_count > 0 {
            "🔴 GHOSTBLOODS FLASH DIGEST"
        } else {
            "🟡 GHOSTBLOODS DIGEST"
        };

        let body = format!(
            "{} alerts detected: {} FLASH, {} PRIORITY\n{}",
            alerts.len(),
            flash_count,
            priority_count,
            alerts.iter().take(3).map(|a| a.title.clone()).collect::<Vec<_>>().join(" • ")
        );

        let _ = app.notification()
            .builder()
            .title(title)
            .body(&body)
            .show();

        record_alert();
    } else {
        // Send individual notifications (up to remaining rate limit)
        let remaining = settings.max_notifications_per_hour as usize - recent_count;
        for alert in alerts.iter().take(remaining) {
            send_single_notification(app, alert);
        }
    }
}

fn send_single_notification(app: &tauri::AppHandle, alert: &SweepAlert) {
    let title = match alert.tier.as_str() {
        "FLASH" => format!("🔴 GHOSTBLOODS FLASH"),
        "PRIORITY" => format!("🟡 GHOSTBLOODS"),
        _ => format!("🔵 GHOSTBLOODS"),
    };

    let body = format!("{}\nSource: {} • Score: {:.0}", alert.title, alert.source, alert.score);

    let _ = app.notification()
        .builder()
        .title(&title)
        .body(&body)
        .show();

    record_alert();
}

fn record_alert() {
    let mut timestamps = ALERT_TIMESTAMPS.lock().unwrap();
    timestamps.push(now_epoch());
    // Keep only last hour
    let cutoff = now_epoch().saturating_sub(3600);
    timestamps.retain(|&t| t > cutoff);
}

// === Mute support ===

pub fn mute_for(_app: &tauri::AppHandle, hours: f64) {
    let until = now_epoch() + (hours * 3600.0) as u64;
    let mut mute = MUTE_UNTIL.lock().unwrap();
    *mute = Some(until);
}

pub fn unmute(_app: &tauri::AppHandle) {
    let mut mute = MUTE_UNTIL.lock().unwrap();
    *mute = None;
}

fn is_muted() -> bool {
    let mute = MUTE_UNTIL.lock().unwrap();
    match *mute {
        Some(until) => now_epoch() < until,
        None => false,
    }
}

fn is_quiet_hours(settings: &NotifierSettings) -> bool {
    if let (Some(start), Some(end)) = (settings.quiet_hours_start, settings.quiet_hours_end) {
        let hour = (now_epoch() / 3600 % 24) as u8;
        if start <= end {
            hour >= start && hour < end
        } else {
            // Wraps midnight (e.g. 22:00 - 07:00)
            hour >= start || hour < end
        }
    } else {
        false
    }
}

// === State persistence ===

pub fn load_state(app: &tauri::AppHandle) -> NotifierState {
    let state_path = get_state_path(app);
    if state_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&state_path) {
            if let Ok(state) = serde_json::from_str(&data) {
                return state;
            }
        }
    }
    NotifierState::default()
}

fn save_state(app: &tauri::AppHandle, state: &NotifierState) {
    let state_path = get_state_path(app);
    if let Some(parent) = state_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(&state_path, json);
    }
}

pub fn save_settings(app: &tauri::AppHandle, settings_json: &str) -> Result<(), Box<dyn std::error::Error>> {
    let settings: NotifierSettings = serde_json::from_str(settings_json)?;
    let mut state = load_state(app);
    state.settings = settings;
    save_state(app, &state);
    Ok(())
}

fn get_state_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir()
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
        .join("notifier-state.json")
}

fn find_project_root() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.join("server.mjs").exists() {
        return cwd;
    }
    // Check parent directories
    let mut dir = cwd.clone();
    for _ in 0..5 {
        if dir.join("server.mjs").exists() {
            return dir;
        }
        if let Some(parent) = dir.parent() {
            dir = parent.to_path_buf();
        } else {
            break;
        }
    }
    cwd
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn chrono_now() -> String {
    // Simple ISO timestamp without chrono dependency
    let epoch = now_epoch();
    format!("{}", epoch)
}
