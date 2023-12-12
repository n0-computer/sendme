//! Command line arguments.
use anyhow::Context;
use clap::{Parser, Subcommand};
use futures::{future, FutureExt, StreamExt};
use iroh_bytes::{
    provider::{handle_connection, EventSender, RequestAuthorizationHandler},
    store::{ImportMode, Store, ExportMode},
    BlobFormat, HashAndFormat, TempTag,
};
use iroh_net::{key::SecretKey, MagicEndpoint};
use walkdir::WalkDir;
use std::{
    any, io,
    net::{SocketAddr, ToSocketAddrs},
    path::{PathBuf, Path, Component},
    str::FromStr,
    sync::Arc,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    select,
};
use tokio_util::sync::CancellationToken;
mod sendme_ticket;
use sendme_ticket::Ticket;
mod collection;
mod get;
mod progress;
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
    pub path: PathBuf,

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

#[derive(Debug)]
struct NoAuth;

#[derive(Debug, Clone)]
struct LogEvents;

impl EventSender for LogEvents {
    fn send(&self, event: iroh_bytes::provider::Event) -> futures::prelude::future::BoxFuture<()> {
        tracing::info!("event: {:?}", event);
        future::ready(()).boxed()
    }
}

impl RequestAuthorizationHandler for NoAuth {
    fn authorize(
        &self,
        token: Option<iroh_bytes::protocol::RequestToken>,
        request: &iroh_bytes::protocol::Request,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        future::ok(()).boxed()
    }
}

fn validate_path_component(component: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!component.contains('/'), "path components must not contain the only correct path separator, /");
    Ok(())
}


/// This function converts an already canonicalized path to a string.
///
/// If `must_be_relative` is true, the function will fail if any component of the path is
/// `Component::RootDir`
///
/// This function will also fail if the path is non canonical, i.e. contains
/// `..` or `.`, or if the path components contain any windows or unix path
/// separators.
pub fn canonicalized_path_to_string(
    path: impl AsRef<Path>,
    must_be_relative: bool,
) -> anyhow::Result<String> {
    let mut path_str = String::new();
    let parts = path
        .as_ref()
        .components()
        .filter_map(|c| match c {
            Component::Normal(x) => {
                let c = match x.to_str() {
                    Some(c) => c,
                    None => return Some(Err(anyhow::anyhow!("invalid character in path"))),
                };

                if !c.contains('/') && !c.contains('\\') {
                    Some(Ok(c))
                } else {
                    Some(Err(anyhow::anyhow!("invalid path component {:?}", c)))
                }
            }
            Component::RootDir => {
                if must_be_relative {
                    Some(Err(anyhow::anyhow!("invalid path component {:?}", c)))
                } else {
                    path_str.push('/');
                    None
                }
            }
            _ => Some(Err(anyhow::anyhow!("invalid path component {:?}", c))),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let parts = parts.join("/");
    path_str.push_str(&parts);
    Ok(path_str)
}


/// Import from a file or directory into the database.
/// 
/// The returned tag always refers to a collection. If the input is a file, this
/// is a collection with a single blob, named like the file.
/// 
/// If the input is a directory, the collection contains all the files in the
/// directory.
async fn import(
    path: PathBuf,
    db: impl iroh_bytes::store::Store,
) -> anyhow::Result<(TempTag, u64)> {
    let path = path.canonicalize()?;
    anyhow::ensure!(path.exists(), "path {} does not exist", path.display());
    let prefix = path
        .components()
        .last()
        .context("path is empty")?
        .as_os_str()
        .to_str()
        .context("not valid utf8")?
        .to_string();
    validate_path_component(&prefix)?;
    let progress = iroh_bytes::util::progress::IgnoreProgressSender::default();
    let root = path.parent().context("context get parent")?;
    // walkdir also works for files
    let files = WalkDir::new(path.clone()).into_iter();
    let data_sources = files
    .map(|entry| {
        let entry = entry?;
        if !entry.file_type().is_file() {
            // Skip symlinks. Directories are handled by WalkDir.
            return Ok(None);
        }
        let path = entry.into_path();
        let relative = path.strip_prefix(&root)?;
        let name = canonicalized_path_to_string(relative, true)?;
        anyhow::Ok(Some((name, path)))
    })
    .filter_map(Result::transpose);
    let data_sources: Vec<anyhow::Result<(String, PathBuf)>> = data_sources.collect::<Vec<_>>();
    let data_sources = data_sources.into_iter().collect::<anyhow::Result<Vec<_>>>()?;
    let streams = futures::stream::iter(data_sources)
        .map(|(name, path)| {
            let db = db.clone();
            let progress = progress.clone();
            async move {
                let (temp_tag, file_size) = db
                    .import_file(
                        path,
                        ImportMode::TryReference,
                        BlobFormat::Raw,
                        progress,
                    )
                    .await?;
                anyhow::Ok((name, temp_tag, file_size))
            }
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>().await;
    let streams = streams.into_iter().collect::<anyhow::Result<Vec<_>>>()?;
    // total size of all files
    let size = streams.iter().map(|(_, _, size)| *size).sum::<u64>();
    // get names and tags
    let (names, tags) = streams.into_iter().map(|(name, tag, _)| (name, tag)).unzip::<_, _, Vec<_>, Vec<_>>();
    // make a collection
    let hashes = tags.iter().map(|tag| *tag.hash()).collect::<Vec<_>>();
    let collection = crate::collection::Collection::from_iter(names.into_iter().zip(hashes));
    let temp_tag = collection.store(&db).await?;
    Ok((temp_tag, size))
}

fn get_export_path(root: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let parts = name.split("/");
    let mut path = root.to_path_buf();
    for part in parts {
        validate_path_component(part)?;
        path.push(part);
    }
    Ok(path)
}

async fn export(db: impl iroh_bytes::store::Store, root: HashAndFormat) -> anyhow::Result<()> {
    let collection = crate::collection::Collection::load(&db, &root.hash).await?;
    let root = std::env::current_dir()?;
    for (name, hash) in collection.iter() {
        let target = get_export_path(&root, name)?;
        db.export(*hash, target, ExportMode::TryReference, |x| Ok(())).await?;
    }
    Ok(())
}

async fn provide(args: ProvideArgs) -> anyhow::Result<()> {
    let secret_key = get_or_create_secret()?;
    // create a magicsocket endpoint
    let endpoint = MagicEndpoint::builder()
        .alpns(vec![iroh_bytes::protocol::ALPN.to_vec()])
        .secret_key(secret_key)
        .bind(args.magic_port)
        .await?;
    // use a flat store - todo: use a partial in mem store instead
    let iroh_data_dir = std::env::current_dir()?.join(".sendme-provide");
    if iroh_data_dir.exists() {
        println!("can not share twice from the same directory");
        std::process::exit(1);
    }
    std::fs::create_dir_all(&iroh_data_dir)?;
    let rt = iroh_bytes::util::runtime::Handle::from_current(1)?;
    let db = iroh_bytes::store::flat::Store::load(
        iroh_data_dir.clone(),
        iroh_data_dir.clone(),
        iroh_data_dir.clone(),
        &rt,
    )
    .await?;
    let auth = Arc::new(NoAuth);
    let path = args.path;
    let (temp_tag, size) = import(path.clone(), db.clone()).await?;
    let hash = *temp_tag.hash();
    println!("imported file {}, {} bytes", path.display(), size);

    // wait for the endpoint to figure out its address before making a ticket
    while endpoint.my_derp().is_none() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // make a ticket
    let addr = endpoint.my_addr().await?;
    let ticket = Ticket::new(addr, hash, BlobFormat::HashSeq)?;
    println!("use");
    println!("sendme get {}", ticket);
    println!("to get this data");
    loop {
        let Some(connecting) = endpoint.accept().await else {
            tracing::info!("no more incoming connections, exiting");
            break;
        };
        let db = db.clone();
        let rt = rt.clone();
        let auth = auth.clone();
        tokio::spawn(handle_connection(connecting, db, LogEvents, auth, rt));
    }
    drop(temp_tag);
    std::fs::remove_dir_all(iroh_data_dir)?;
    Ok(())
}

async fn get(args: GetArgs) -> anyhow::Result<()> {
    let secret_key = get_or_create_secret()?;
    let endpoint = MagicEndpoint::builder()
        .alpns(vec![])
        .secret_key(secret_key)
        .bind(args.magic_port)
        .await?;
    let iroh_data_dir = std::env::current_dir()?.join(".sendme-get");
    let rt = iroh_bytes::util::runtime::Handle::from_current(1)?;
    let db = iroh_bytes::store::flat::Store::load(
        iroh_data_dir.clone(),
        iroh_data_dir.clone(),
        iroh_data_dir.clone(),
        &rt,
    )
    .await?;
    let ticket = sendme_ticket::Ticket::from_str(&args.ticket)?;
    let addr = ticket.node_addr().clone();
    let connection = endpoint.connect(addr, &iroh_bytes::protocol::ALPN).await?;
    let hash_and_format = HashAndFormat {
        hash: ticket.hash(),
        format: ticket.format(),
    };
    let progress = iroh_bytes::util::progress::IgnoreProgressSender::default();
    get::get(&db, connection, &hash_and_format, progress).await?;
    export(db, hash_and_format).await?;
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
