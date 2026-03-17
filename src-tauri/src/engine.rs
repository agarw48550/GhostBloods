// Engine lifecycle — manages the Node.js Crucix server as a child process

use std::sync::Mutex;
use std::process::Stdio;
use tauri::Manager;

static ENGINE_PID: Mutex<Option<u32>> = Mutex::new(None);

/// Start the Crucix Node.js engine as a child process
pub async fn start_engine(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Check if already running
    {
        let pid = ENGINE_PID.lock().unwrap();
        if pid.is_some() {
            // Check if it's actually alive
            if is_port_open().await {
                return Ok(());
            }
        }
    }

    let resource_dir = app.path().resource_dir()
        .unwrap_or_else(|_| std::env::current_dir().unwrap());

    // Try to find the project root (where server.mjs lives)
    let project_root = find_project_root(&resource_dir);

    println!("[GhostBloods] Starting engine from: {:?}", project_root);

    let child = tokio::process::Command::new("node")
        .arg("server.mjs")
        .current_dir(&project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let pid = child.id().unwrap_or(0);
    {
        let mut engine_pid = ENGINE_PID.lock().unwrap();
        *engine_pid = Some(pid);
    }

    println!("[GhostBloods] Engine started with PID: {}", pid);

    // Detach stdout/stderr reader to avoid blocking
    tokio::spawn(async move {
        let _ = child.wait_with_output().await;
        println!("[GhostBloods] Engine process exited");
        let mut engine_pid = ENGINE_PID.lock().unwrap();
        *engine_pid = None;
    });

    // Wait for the server to be ready (poll port 3117)
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if is_port_open().await {
            println!("[GhostBloods] Engine is ready on port 3117");
            return Ok(());
        }
    }

    Err("Engine failed to start within 15 seconds".into())
}

/// Stop the Crucix Node.js engine
pub async fn stop_engine(_app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let pid = {
        let mut engine_pid = ENGINE_PID.lock().unwrap();
        engine_pid.take()
    };

    if let Some(pid) = pid {
        println!("[GhostBloods] Stopping engine PID: {}", pid);

        // Send SIGTERM via kill command
        let _ = tokio::process::Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .output()
            .await;

        // Wait a moment for graceful shutdown
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Force kill if still alive
        if is_port_open().await {
            let _ = tokio::process::Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .output()
                .await;

            // Also kill any stray node processes on port 3117
            let _ = tokio::process::Command::new("sh")
                .arg("-c")
                .arg("lsof -ti:3117 | xargs kill -9 2>/dev/null")
                .output()
                .await;
        }

        println!("[GhostBloods] Engine stopped");
    }

    Ok(())
}

/// Check if the engine is running by testing port 3117
async fn is_port_open() -> bool {
    match tokio::net::TcpStream::connect("127.0.0.1:3117").await {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Find the project root directory (where server.mjs exists)
fn find_project_root(start: &std::path::Path) -> std::path::PathBuf {
    // Check common locations
    let candidates = [
        start.to_path_buf(),
        start.join(".."),
        start.join("../.."),
        std::env::current_dir().unwrap_or_default(),
    ];

    for candidate in &candidates {
        if candidate.join("server.mjs").exists() {
            return candidate.clone();
        }
    }

    // Fallback: current dir
    std::env::current_dir().unwrap_or_else(|_| start.to_path_buf())
}

pub fn is_engine_running() -> bool {
    let pid = ENGINE_PID.lock().unwrap();
    pid.is_some()
}
