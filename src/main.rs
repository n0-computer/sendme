//! Command line arguments.
use anyhow::Context;
use clap::{Parser, Subcommand};
use iroh_net::{key::SecretKey, magic_endpoint::get_remote_node_id, MagicEndpoint, NodeAddr};
use std::{
    any, io,
    net::{SocketAddr, ToSocketAddrs},
    str::FromStr,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    select,
};
use tokio_util::sync::CancellationToken;

/// Send a file or directory between two machines, using blake3 verified streaming.
///
/// For all subcommands, you can specify a secret key using the IROH_SECRET
/// environment variable. If you don't, a random one will be generated.
///
/// You can also specify a port for the magicsocket. If you don't, a random one
/// will be chosen.
#[derive(Parser, Debug)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Provide a file or directory.
    Provide(ProvideArgs),

    /// Get a file or directory.
    Get(GetArgs),
}

#[derive(Parser, Debug)]
pub struct ProvideArgs {
    /// Path to the file or directory to provide.
    ///
    /// The last component of the path will be used as the name of the data
    /// being shared.
    pub path: String,

    /// The port for the magic socket to listen on.
    ///
    /// Defauls to a random free port, but it can be useful to specify a fixed
    /// port, e.g. to configure a firewall rule.
    #[clap(long, default_value_t = 0)]
    pub magic_port: u16,
}

#[derive(Parser, Debug)]
pub struct GetArgs {
    /// The ticket to use to connect to the provider.
    pub ticket: String,

    /// The port to use for the magicsocket. Random by default.
    #[clap(long, default_value_t = 0)]
    pub magic_port: u16,
}

/// Get the secret key or generate a new one.
///
/// Print the secret key to stderr if it was generated, so the user can save it.
fn get_or_create_secret() -> anyhow::Result<SecretKey> {
    match std::env::var("IROH_SECRET") {
        Ok(secret) => SecretKey::from_str(&secret).context("invalid secret"),
        Err(_) => {
            let key = SecretKey::generate();
            eprintln!("using secret key {}", key);
            Ok(key)
        }
    }
}

async fn provide(_args: ProvideArgs) -> anyhow::Result<()> {
    let _secret = get_or_create_secret()?;
    Ok(())
}

async fn get(_args: GetArgs) -> anyhow::Result<()> {
    let _secret = get_or_create_secret()?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let res = match args.command {
        Commands::Provide(args) => provide(args).await,
        Commands::Get(args) => get(args).await,
    };
    match res {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1)
        }
    }
}
