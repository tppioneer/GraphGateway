//! CLI argument parsing.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;

#[derive(Parser)]
#[command(
    name = "graphgateway",
    version,
    about = "GraphGateway — code graph workspace router"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Start the GraphGateway server.
    Serve {
        /// Run in owned-sidecar mode (required for MVP).
        #[arg(long, default_value_t = true)]
        owned_sidecar: bool,

        /// Listen address.  Always bound to loopback regardless of IP.
        /// Use port 0 for automatic port assignment.
        #[arg(long, default_value = "127.0.0.1:0")]
        listen: SocketAddr,
    },
    /// Print version information.
    Version,
}

pub(crate) fn parse() -> Result<Cli, clap::Error> {
    Cli::try_parse()
}
