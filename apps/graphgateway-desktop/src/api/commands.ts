import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// TypeScript mirrors of graphgateway-types SidecarSnapshot & SidecarState
// ---------------------------------------------------------------------------

export type SidecarState =
  | "stopped"
  | "starting"
  | "ready"
  | "stopping"
  | "failed";

export interface SidecarSnapshot {
  state: SidecarState;
  pid: number | null;
  endpoint: string | null;
  server_version: string | null;
  api_version: string | null;
  started_at: string | null;
  uptime_ms: number | null;
  last_error: string | null;
  desktop_version: string | null;
}

// ---------------------------------------------------------------------------
// Tauri Commands (all go through invoke → Rust backend → graphgateway-client)
// ---------------------------------------------------------------------------

export async function sidecarStatus(): Promise<SidecarSnapshot> {
  return invoke<SidecarSnapshot>("sidecar_status");
}

export async function sidecarStart(): Promise<SidecarSnapshot> {
  return invoke<SidecarSnapshot>("sidecar_start");
}

export async function sidecarRestart(): Promise<SidecarSnapshot> {
  return invoke<SidecarSnapshot>("sidecar_restart");
}

export async function sidecarStop(): Promise<SidecarSnapshot> {
  return invoke<SidecarSnapshot>("sidecar_stop");
}
