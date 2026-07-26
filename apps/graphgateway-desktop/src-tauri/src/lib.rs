mod commands;
mod sidecar;

use sidecar::SidecarManager;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{watch, Mutex};

/// Optional production diagnostic sink used by unattended launches and
/// packaged smoke tests. The file contains the same redacted snapshot exposed
/// to the frontend; startup tokens are never included.
fn write_startup_diagnostic(snapshot: &graphgateway_types::SidecarSnapshot) {
    let Ok(path) = std::env::var("GRAPHGATEWAY_STARTUP_DIAGNOSTIC_FILE") else {
        return;
    };
    match serde_json::to_vec(snapshot) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&path, json) {
                tracing::warn!(%error, %path, "failed to write startup diagnostic");
            }
        }
        Err(error) => tracing::warn!(%error, "failed to serialize startup diagnostic"),
    }
}

/// Application state shared across all Tauri commands.
struct AppState {
    /// The sidecar manager, protected by a Tokio async mutex.
    sidecar: Arc<Mutex<SidecarManager>>,
    /// Watch channel for the latest SidecarSnapshot.
    snapshot_rx: Mutex<watch::Receiver<graphgateway_types::SidecarSnapshot>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (mgr, snapshot_rx) = SidecarManager::new();
    let sidecar = Arc::new(Mutex::new(mgr));

    let sidecar_setup = sidecar.clone();
    let snapshot_rx = Mutex::new(snapshot_rx);

    tauri::Builder::default()
        .manage(AppState {
            sidecar: sidecar.clone(),
            snapshot_rx,
        })
        .invoke_handler(tauri::generate_handler![
            commands::sidecar_status,
            commands::sidecar_start,
            commands::sidecar_restart,
            commands::sidecar_stop,
        ])
        .setup(move |_app| {
            // Auto-start the sidecar in the background and run periodic health
            // checks.  Both run on Tauri's async runtime.
            let mgr = sidecar_setup.clone();
            tauri::async_runtime::spawn(async move {
                tracing::info!("auto-starting sidecar...");
                let mut guard = mgr.lock().await;
                if let Err(e) = guard.start().await {
                    tracing::warn!(error = %e, "auto-start failed (user can retry)");
                    write_startup_diagnostic(&guard.build_snapshot());
                }
                drop(guard);

                // Periodic health check loop.
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let mut guard = mgr.lock().await;
                    guard.check_health().await;
                    drop(guard);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let app_handle = window.app_handle();
                // Only shut down when the last window is destroyed.
                if app_handle.webview_windows().len() <= 1 {
                    tracing::info!("last window destroyed, shutting down sidecar");
                    let state: tauri::State<'_, AppState> = window.state();
                    let mgr = state.sidecar.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut guard = mgr.lock().await;
                        let _ = guard.stop().await;
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running GraphGateway Desktop");
}
