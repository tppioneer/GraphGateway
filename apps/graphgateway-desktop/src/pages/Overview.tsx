import {
  useSidecarStatus,
  useSidecarStart,
  useSidecarRestart,
  useSidecarStop,
} from "../state/queries";
import type { SidecarState, SidecarSnapshot } from "../api/commands";
import "./Overview.css";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function stateClass(state: SidecarState): string {
  switch (state) {
    case "ready":
      return "state-ready";
    case "starting":
    case "stopping":
      return "state-transition";
    case "failed":
      return "state-failed";
    default:
      return "state-stopped";
  }
}

function uptime(ms: number | null | undefined): string {
  if (ms == null) return "—";
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

// ---------------------------------------------------------------------------
// Stat tile
// ---------------------------------------------------------------------------

function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat-tile">
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

export default function Overview() {
  const { data, isLoading, isError, error, refetch } = useSidecarStatus();
  const start = useSidecarStart();
  const restart = useSidecarRestart();
  const stop = useSidecarStop();

  const snapshot: SidecarSnapshot | undefined = data;

  return (
    <section className="overview">
      <div className="overview-header">
        <h2>Sidecar Status</h2>
        <div className="overview-actions">
          <button
            disabled={snapshot?.state === "starting" || snapshot?.state === "ready"}
            onClick={() => start.mutate()}
          >
            Start
          </button>
          <button
            disabled={snapshot?.state !== "ready"}
            onClick={() => restart.mutate()}
          >
            Restart
          </button>
          <button
            disabled={snapshot?.state !== "ready"}
            onClick={() => stop.mutate()}
          >
            Stop
          </button>
          <button onClick={() => refetch()}>Refresh</button>
        </div>
      </div>

      {isLoading && <p className="overview-msg">Loading sidecar status…</p>}

      {isError && (
        <p className="overview-msg overview-error">
          Failed to read status: {String(error)}
        </p>
      )}

      {snapshot && (
        <>
          <div className="status-bar">
            <span className={`status-badge ${stateClass(snapshot.state)}`}>
              {snapshot.state.toUpperCase()}
            </span>
            {snapshot.last_error && (
              <span className="last-error" title={snapshot.last_error}>
                ⚠ {snapshot.last_error}
              </span>
            )}
          </div>

          <div className="stats-grid">
            <StatTile
              label="Desktop Version"
              value={snapshot.desktop_version ?? "—"}
            />
            <StatTile
              label="Server Version"
              value={snapshot.server_version ?? "—"}
            />
            <StatTile label="API Version" value={snapshot.api_version ?? "—"} />
            <StatTile
              label="PID"
              value={snapshot.pid != null ? String(snapshot.pid) : "—"}
            />
            <StatTile label="Endpoint" value={snapshot.endpoint ?? "—"} />
            <StatTile label="Uptime" value={uptime(snapshot.uptime_ms)} />
            <StatTile label="Started At" value={snapshot.started_at ?? "—"} />
          </div>

          {(start.isPending || restart.isPending || stop.isPending) && (
            <p className="overview-msg">
              {start.isPending
                ? "Starting sidecar…"
                : restart.isPending
                  ? "Restarting sidecar…"
                  : "Stopping sidecar…"}
            </p>
          )}

          {(start.isError || restart.isError || stop.isError) && (
            <p className="overview-msg overview-error">
              {start.isError
                ? `Start failed: ${start.error}`
                : restart.isError
                  ? `Restart failed: ${restart.error}`
                  : `Stop failed: ${stop.error}`}
            </p>
          )}
        </>
      )}
    </section>
  );
}
