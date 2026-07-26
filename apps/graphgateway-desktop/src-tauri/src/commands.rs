//! Tauri command handlers — async wrappers over [`SidecarManager`].
//!
//! All commands are `async`.  The SidecarManager is protected by a
//! `tokio::sync::Mutex` so lifecycle operations are serialized without
//! blocking the Tauri IPC thread.
//!
//! No `block_on`, no `Handle::current()`, no `std::sync::Mutex`.

use graphgateway_types::SidecarSnapshot;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub(crate) async fn sidecar_status(state: State<'_, AppState>) -> Result<SidecarSnapshot, String> {
    let snap_rx = state.snapshot_rx.lock().await;
    let mut snap = snap_rx.borrow().clone();
    snap.desktop_version = Some(env!("CARGO_PKG_VERSION").to_string());
    Ok(snap)
}

#[tauri::command]
pub(crate) async fn sidecar_start(state: State<'_, AppState>) -> Result<SidecarSnapshot, String> {
    let mut mgr = state.sidecar.lock().await;
    mgr.start()
        .await
        .map_err(|e| format!("start failed: {e}"))?;
    let mut snap = mgr.build_snapshot();
    snap.desktop_version = Some(env!("CARGO_PKG_VERSION").to_string());
    Ok(snap)
}

#[tauri::command]
pub(crate) async fn sidecar_restart(state: State<'_, AppState>) -> Result<SidecarSnapshot, String> {
    let mut mgr = state.sidecar.lock().await;
    mgr.restart()
        .await
        .map_err(|e| format!("restart failed: {e}"))?;
    let mut snap = mgr.build_snapshot();
    snap.desktop_version = Some(env!("CARGO_PKG_VERSION").to_string());
    Ok(snap)
}

#[tauri::command]
pub(crate) async fn sidecar_stop(state: State<'_, AppState>) -> Result<SidecarSnapshot, String> {
    let mut mgr = state.sidecar.lock().await;
    mgr.stop().await.map_err(|e| format!("stop failed: {e}"))?;
    let mut snap = mgr.build_snapshot();
    snap.desktop_version = Some(env!("CARGO_PKG_VERSION").to_string());
    Ok(snap)
}
