//! GraphGateway server — entry point.
//!
//! Supports:
//! - `graphgateway.exe serve --owned-sidecar --listen 127.0.0.1:0`
//! - `graphgateway.exe version`

mod cli;
mod handshake;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::oneshot;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use graphgateway_rest::{AppState, build_router};

#[tokio::main]
async fn main() {
    // Structured tracing → stderr only.  Stdout is reserved for the
    // sidecar control protocol (ready message).
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .json(),
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "graphgateway=info".into()),
        )
        .init();

    let args = match cli::parse() {
        Ok(a) => a,
        Err(e) => {
            eprint!("{e}");
            std::process::exit(2);
        }
    };

    match args.command {
        cli::Command::Serve {
            owned_sidecar,
            listen,
        } => {
            if !owned_sidecar {
                // Shared Service and Remote modes are not yet implemented.
                // The CLI enforces --owned-sidecar for MVP.
            }
            if let Err(e) = run_serve(listen).await {
                tracing::error!(error = %e, "server exited with error");
                std::process::exit(1);
            }
        }
        cli::Command::Version => {
            println!("graphgateway {}", env!("CARGO_PKG_VERSION"));
        }
    }
}

async fn run_serve(listen: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    // Read startup configuration from stdin (one JSON line).
    let config = handshake::read_startup_config()?;

    // Bind the TCP listener.  We always bind to 127.0.0.1 regardless of
    // what --listen says, for defense-in-depth.
    let bind_addr = SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), listen.port());

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| {
            tracing::error!(addr = %bind_addr, error = %e, "failed to bind");
            e
        })?;

    let local_addr = listener.local_addr()?;
    let endpoint = format!("http://{local_addr}");

    // Graceful shutdown channel.
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let state = Arc::new(AppState {
        token: config.access_token.clone(),
        start_time: Instant::now(),
        endpoint: endpoint.clone(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        shutdown_tx: Mutex::new(Some(shutdown_tx)),
    });

    let app = build_router(Arc::clone(&state));

    // Emit ready message on stdout (NOT stderr — stdout is the control channel).
    handshake::emit_ready(&endpoint, env!("CARGO_PKG_VERSION"), std::process::id())?;

    tracing::info!(
        endpoint = %endpoint,
        pid = std::process::id(),
        "sidecar ready"
    );

    // Serve with graceful shutdown.
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
            tracing::info!("graceful shutdown initiated");
        })
        .await?;

    tracing::info!("server stopped");
    Ok(())
}
