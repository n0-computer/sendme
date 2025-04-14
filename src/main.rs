//! Command line arguments.

use std::{
    collections::{btree_map::Range, BTreeMap},
    fmt::{Display, Formatter},
    net::{SocketAddrV4, SocketAddrV6},
    path::{Component, Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use blobs2::{
    api::{
        blobs::{
            ExportMode, ExportPath, ExportProgress, ImportMode, ImportPathOptions, ImportProgress,
        },
        Store,
    },
    format::collection::Collection,
    get::{
        fsm::{AtBlobHeaderNextError, DecodeError},
        request::get_hash_seq_and_sizes,
        Stats,
    },
    net_protocol::Blobs,
    protocol::{GetRequest, RangeSpecSeq},
    provider::{self, Event},
    store::fs::FsStore,
    ticket::BlobTicket,
    util::temp_tag::{self, TempTag},
    BlobFormat, Hash, HashAndFormat,
};
use clap::{
    error::{ContextKind, ErrorKind},
    CommandFactory, Parser, Subcommand,
};
use console::style;
use data_encoding::HEXLOWER;
use futures_buffered::BufferedStreamExt;
use indicatif::{
    HumanBytes, HumanDuration, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle,
};
use iroh::{
    discovery::{dns::DnsDiscovery, pkarr::PkarrPublisher},
    Endpoint, NodeAddr, RelayMap, RelayMode, RelayUrl, SecretKey,
};
use n0_future::{pin, stream, task::AbortOnDropHandle, StreamExt};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc};
use tracing::{error, trace};
use walkdir::WalkDir;

/// Send a file or directory between two machines, using blake3 verified streaming.
///
/// For all subcommands, you can specify a secret key using the IROH_SECRET
/// environment variable. If you don't, a random one will be generated.
///
/// You can also specify a port for the magicsocket. If you don't, a random one
/// will be chosen.
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    #[default]
    Hex,
    Cid,
}

impl FromStr for Format {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "hex" => Ok(Format::Hex),
            "cid" => Ok(Format::Cid),
            _ => Err(anyhow::anyhow!("invalid format")),
        }
    }
}

impl Display for Format {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Hex => write!(f, "hex"),
            Format::Cid => write!(f, "cid"),
        }
    }
}

fn print_hash(hash: &Hash, format: Format) -> String {
    match format {
        Format::Hex => hash.to_hex().to_string(),
        Format::Cid => hash.to_string(),
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Send a file or directory.
    Send(SendArgs),

    /// Receive a file or directory.
    #[clap(visible_alias = "recv")]
    Receive(ReceiveArgs),
}

#[derive(Parser, Debug)]
pub struct CommonArgs {
    /// The IPv4 address that magicsocket will listen on.
    ///
    /// If None, defaults to a random free port, but it can be useful to specify a fixed
    /// port, e.g. to configure a firewall rule.
    #[clap(long, default_value = None)]
    pub magic_ipv4_addr: Option<SocketAddrV4>,

    /// The IPv6 address that magicsocket will listen on.
    ///
    /// If None, defaults to a random free port, but it can be useful to specify a fixed
    /// port, e.g. to configure a firewall rule.
    #[clap(long, default_value = None)]
    pub magic_ipv6_addr: Option<SocketAddrV6>,

    #[clap(long, default_value_t = Format::Hex)]
    pub format: Format,

    #[clap(short = 'v', long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress progress bars.
    #[clap(long, default_value_t = false)]
    pub no_progress: bool,

    /// The relay URL to use as a home relay,
    ///
    /// Can be set to "disable" to disable relay servers and "default"
    /// to configure default servers.
    #[clap(long, default_value_t = RelayModeOption::Default)]
    pub relay: RelayModeOption,

    #[clap(long)]
    pub show_secret: bool,
}

/// Available command line options for configuring relays.
#[derive(Clone, Debug)]
pub enum RelayModeOption {
    /// Disables relays altogether.
    Disabled,
    /// Uses the default relay servers.
    Default,
    /// Uses a single, custom relay server by URL.
    Custom(RelayUrl),
}

impl FromStr for RelayModeOption {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "disabled" => Ok(Self::Disabled),
            "default" => Ok(Self::Default),
            _ => Ok(Self::Custom(RelayUrl::from_str(s)?)),
        }
    }
}

impl Display for RelayModeOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => f.write_str("disabled"),
            Self::Default => f.write_str("default"),
            Self::Custom(url) => url.fmt(f),
        }
    }
}

impl From<RelayModeOption> for RelayMode {
    fn from(value: RelayModeOption) -> Self {
        match value {
            RelayModeOption::Disabled => RelayMode::Disabled,
            RelayModeOption::Default => RelayMode::Default,
            RelayModeOption::Custom(url) => RelayMode::Custom(RelayMap::from_url(url)),
        }
    }
}

#[derive(Parser, Debug)]
pub struct SendArgs {
    /// Path to the file or directory to send.
    ///
    /// The last component of the path will be used as the name of the data
    /// being shared.
    pub path: PathBuf,

    /// What type of ticket to use.
    ///
    /// Use "id" for the shortest type only including the node ID,
    /// "addresses" to only add IP addresses without a relay url,
    /// "relay" to only add a relay address, and leave the option out
    /// to use the biggest type of ticket that includes both relay and
    /// address information.
    ///
    /// Generally, the more information the higher the likelyhood of
    /// a successful connection, but also the bigger a ticket to connect.
    ///
    /// This is most useful for debugging which methods of connection
    /// establishment work well.
    #[clap(long, default_value_t = AddrInfoOptions::RelayAndAddresses)]
    pub ticket_type: AddrInfoOptions,

    #[clap(flatten)]
    pub common: CommonArgs,
}

#[derive(Parser, Debug)]
pub struct ReceiveArgs {
    /// The ticket to use to connect to the sender.
    pub ticket: BlobTicket,

    #[clap(flatten)]
    pub common: CommonArgs,
}

/// Options to configure what is included in a [`NodeAddr`]
#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    Default,
    Debug,
    derive_more::Display,
    derive_more::FromStr,
    Serialize,
    Deserialize,
)]
pub enum AddrInfoOptions {
    /// Only the Node ID is added.
    ///
    /// This usually means that iroh-dns discovery is used to find address information.
    #[default]
    Id,
    /// Includes the Node ID and both the relay URL, and the direct addresses.
    RelayAndAddresses,
    /// Includes the Node ID and the relay URL.
    Relay,
    /// Includes the Node ID and the direct addresses.
    Addresses,
}

fn apply_options(addr: &mut NodeAddr, opts: AddrInfoOptions) {
    match opts {
        AddrInfoOptions::Id => {
            addr.direct_addresses.clear();
            addr.relay_url = None;
        }
        AddrInfoOptions::RelayAndAddresses => {
            // nothing to do
        }
        AddrInfoOptions::Relay => {
            addr.direct_addresses.clear();
        }
        AddrInfoOptions::Addresses => {
            addr.relay_url = None;
        }
    }
}

/// Get the secret key or generate a new one.
///
/// Print the secret key to stderr if it was generated, so the user can save it.
fn get_or_create_secret(print: bool) -> anyhow::Result<SecretKey> {
    match std::env::var("IROH_SECRET") {
        Ok(secret) => SecretKey::from_str(&secret).context("invalid secret"),
        Err(_) => {
            let key = SecretKey::generate(rand::rngs::OsRng);
            if print {
                eprintln!("using secret key {}", key);
            }
            Ok(key)
        }
    }
}

fn validate_path_component(component: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !component.contains('/'),
        "path components must not contain the only correct path separator, /"
    );
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
    db: &Store,
    mp: &mut MultiProgress,
) -> anyhow::Result<(TempTag, u64, Collection)> {
    let parallelism = num_cpus::get();
    let path = path.canonicalize()?;
    anyhow::ensure!(path.exists(), "path {} does not exist", path.display());
    let root = path.parent().context("context get parent")?;
    // walkdir also works for files, so we don't need to special case them
    let files = WalkDir::new(path.clone()).into_iter();
    // flatten the directory structure into a list of (name, path) pairs.
    // ignore symlinks.
    let data_sources: Vec<(String, PathBuf)> = files
        .map(|entry| {
            let entry = entry?;
            if !entry.file_type().is_file() {
                // Skip symlinks. Directories are handled by WalkDir.
                return Ok(None);
            }
            let path = entry.into_path();
            let relative = path.strip_prefix(root)?;
            let name = canonicalized_path_to_string(relative, true)?;
            anyhow::Ok(Some((name, path)))
        })
        .filter_map(Result::transpose)
        .collect::<anyhow::Result<Vec<_>>>()?;
    // import all the files, using num_cpus workers, return names and temp tags
    let op = mp.add(ProgressBar::hidden());
    op.set_style(
        ProgressStyle::with_template(
            "{msg}{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len}",
        )?
        .progress_chars("#>-"),
    );
    op.set_message(format!("importing {} files", data_sources.len()));
    op.set_length(data_sources.len() as u64);
    let mut names_and_tags =  n0_future::stream::iter(data_sources)
        .map(|(name, path)| {
            let db = db.clone();
            let pb = mp.add(ProgressBar::hidden());
            let op = op.clone();
            async move {
                op.inc(1);
                pb.set_style(
                    ProgressStyle::with_template(
                        "{msg}{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
                    )?
                    .progress_chars("#>-"),
                );
                pb.set_message(format!("copying {name}"));
                let import = db
                    .add_path_with_opts(ImportPathOptions {
                        path,
                        mode: ImportMode::TryReference,
                        format: BlobFormat::Raw,
                    });
                let mut stream = import.stream().await?;
                let mut item_size = 0;
                let temp_tag = loop {
                    let item = stream.next().await.context("import stream ended without a tag")?;
                    trace!("importing {name} {item:?}");
                    match item {
                        ImportProgress::Size(size) => {
                            item_size = size;
                            pb.set_length(size);
                        }
                        ImportProgress::CopyProgress(offset) => {
                            pb.set_position(offset);
                        }
                        ImportProgress::CopyDone => {
                            pb.set_position(0);
                            pb.set_message(format!("computing outboard {name}"));
                        }
                        ImportProgress::OutboardProgress(offset ) => {
                            pb.set_position(offset);
                        }
                        ImportProgress::Error(cause ) => {
                            pb.finish_and_clear();
                            anyhow::bail!("error importing {}: {}", name, cause);
                        }
                        ImportProgress::Done(tt ) => {
                            pb.finish_and_clear();
                            break tt;
                        }
                    }
                };
                anyhow::Ok((name, temp_tag, item_size))
            }
        })
        // .then(|x| x)
        .buffered_unordered(parallelism)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;
    op.finish_and_clear();
    names_and_tags.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
    // total size of all files
    let size = names_and_tags.iter().map(|(_, _, size)| *size).sum::<u64>();
    // collect the (name, hash) tuples into a collection
    // we must also keep the tags around so the data does not get gced.
    let (collection, tags) = names_and_tags
        .into_iter()
        .map(|(name, tag, _)| ((name, *tag.hash()), tag))
        .unzip::<_, _, Collection, Vec<_>>();
    let temp_tag = collection.clone().store(&db).await?;
    // now that the collection is stored, we can drop the tags
    // data is protected by the collection
    drop(tags);
    Ok((temp_tag, size, collection))
}

fn get_export_path(root: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let parts = name.split('/');
    let mut path = root.to_path_buf();
    for part in parts {
        validate_path_component(part)?;
        path.push(part);
    }
    Ok(path)
}

async fn export(db: &Store, collection: Collection, mp: &mut MultiProgress) -> anyhow::Result<()> {
    let root = std::env::current_dir()?;
    for (name, hash) in collection.iter() {
        let target = get_export_path(&root, name)?;
        if target.exists() {
            eprintln!(
                "target {} already exists. Export stopped.",
                target.display()
            );
            eprintln!("You can remove the file or directory and try again. The download will not be repeated.");
            anyhow::bail!("target {} already exists", target.display());
        }
        let mut stream = db
            .export_with_opts(ExportPath {
                hash: *hash,
                target,
                mode: ExportMode::TryReference,
            })
            .stream()
            .await;
        let pb = mp.add(ProgressBar::hidden());
        pb.set_style(
            ProgressStyle::with_template(
                "{msg}{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
            )?
            .progress_chars("#>-"),
        );
        pb.set_message(format!("exporting {}", name));
        while let Some(item) = stream.next().await {
            match item {
                ExportProgress::Size(size) => {
                    pb.set_length(size);
                }
                ExportProgress::CopyProgress(offset) => {
                    pb.set_position(offset);
                }
                ExportProgress::Done => {
                    pb.finish_and_clear();
                }
                ExportProgress::Error(cause) => {
                    pb.finish_and_clear();
                    anyhow::bail!("error exporting {}: {}", name, cause);
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct PerConnectionProgress {
    main: ProgressBar,
    requests: BTreeMap<u64, ProgressBar>,
}

async fn show_provide_progress(
    mp: MultiProgress,
    mut recv: mpsc::Receiver<provider::Event>,
) -> anyhow::Result<()> {
    let mut connections = BTreeMap::new();
    while let Some(item) = recv.recv().await {
        trace!("got event {item:?}");
        match item {
            Event::ClientConnected {
                connection_id,
                node_id,
            } => {
                let pb = mp.add(ProgressBar::hidden());
                pb.set_style(
                    indicatif::ProgressStyle::default_bar()
                        .template("{msg}") // Only display the message
                        .unwrap(),
                );
                pb.set_message(format!("{} {}", node_id, connection_id));
                connections.insert(
                    connection_id,
                    PerConnectionProgress {
                        main: pb,
                        requests: BTreeMap::new(),
                    },
                );
            }
            Event::ConnectionClosed { connection_id } => {
                let Some(connection) = connections.remove(&connection_id) else {
                    error!("got close for unknown connection {connection_id}");
                    continue;
                };
                for pb in connection.requests.values() {
                    pb.finish_and_clear();
                }
                connection.main.finish_and_clear();
            }
            Event::GetRequestReceived {
                connection_id,
                request_id,
                hash,
                ..
            } => {
                let pb = mp.add(ProgressBar::hidden());
                pb.set_style(
                    ProgressStyle::with_template(
                        "{msg}{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
                    )?
                    .progress_chars("#>-"),
                );
                pb.set_message(format!("{} {}", request_id, hash));
                let Some(connection) = connections.get_mut(&connection_id) else {
                    error!("got request for unknown connection {connection_id}");
                    continue;
                };
                connection.requests.insert(request_id, pb);
            }
            Event::TransferStarted {
                connection_id,
                request_id,
                hash,
                size,
                index,
            } => {
                let Some(connection) = connections.get_mut(&connection_id) else {
                    error!("got request for unknown connection {connection_id}");
                    continue;
                };
                let Some(pb) = connection.requests.get_mut(&request_id) else {
                    error!("got update for unknown request {request_id}");
                    continue;
                };
                pb.set_message(format!("    {} {} {}", request_id, index, hash.fmt_short()));
                pb.set_length(size);
            }
            Event::TransferProgress {
                connection_id,
                request_id,
                end_offset,
                ..
            } => {
                let Some(connection) = connections.get_mut(&connection_id) else {
                    error!("got request for unknown connection {connection_id}");
                    continue;
                };
                let Some(pb) = connection.requests.get_mut(&request_id) else {
                    error!("got update for unknown request {request_id}");
                    continue;
                };
                pb.set_position(end_offset);
            }
            Event::TransferCompleted {
                connection_id,
                request_id,
                stats,
            } => {
                if let Some(msg) = connections.get_mut(&connection_id) {
                    if let Some(pb) = msg.requests.remove(&request_id) {
                        // todo: show stats and hide after a delay
                        pb.finish_and_clear();
                    }
                }
            }
            Event::TransferAborted {
                connection_id,
                request_id,
                stats,
            } => {
                if let Some(msg) = connections.get_mut(&connection_id) {
                    if let Some(pb) = msg.requests.remove(&request_id) {
                        // todo: show stats and hide after a delay
                        pb.finish_and_clear();
                    }
                }
            }
        }
    }
    Ok(())
}

async fn send(args: SendArgs) -> anyhow::Result<()> {
    let secret_key = get_or_create_secret(args.common.verbose > 0)?;
    if args.common.show_secret {
        eprintln!("using secret key {}", secret_key);
    }
    // create a magicsocket endpoint
    let mut builder = Endpoint::builder()
        .alpns(vec![blobs2::protocol::ALPN.to_vec()])
        .secret_key(secret_key)
        .relay_mode(args.common.relay.into());
    if args.ticket_type == AddrInfoOptions::Id {
        builder =
            builder.add_discovery(|secret_key| Some(PkarrPublisher::n0_dns(secret_key.clone())));
    }
    if let Some(addr) = args.common.magic_ipv4_addr {
        builder = builder.bind_addr_v4(addr);
    }
    if let Some(addr) = args.common.magic_ipv6_addr {
        builder = builder.bind_addr_v6(addr);
    }

    // use a flat store - todo: use a partial in mem store instead
    let suffix = rand::thread_rng().gen::<[u8; 16]>();
    let cwd = std::env::current_dir()?;
    let blobs_data_dir = cwd.join(format!(".sendme-send-{}", HEXLOWER.encode(&suffix)));
    if blobs_data_dir.exists() {
        println!(
            "can not share twice from the same directory: {}",
            cwd.display(),
        );
        std::process::exit(1);
    }

    let mut mp = MultiProgress::new();
    let mp2 = mp.clone();
    let path = args.path;
    let path2 = path.clone();
    let blobs_data_dir2 = blobs_data_dir.clone();
    let (progress_tx, progress_rx) = mpsc::channel(32);
    let progress = AbortOnDropHandle::new(n0_future::task::spawn(show_provide_progress(
        mp2,
        progress_rx,
    )));
    let setup = async move {
        tokio::fs::create_dir_all(&blobs_data_dir2).await?;

        let endpoint = builder.bind().await?;
        let draw_target = if args.common.no_progress {
            ProgressDrawTarget::hidden()
        } else {
            ProgressDrawTarget::stderr()
        };
        mp.set_draw_target(draw_target);
        let store = FsStore::load(&blobs_data_dir2).await?;
        let blobs = Blobs::new(&store, endpoint.clone(), Some(progress_tx));

        let router = iroh::protocol::Router::builder(endpoint)
            .accept(blobs2::ALPN, blobs.clone())
            .spawn()
            .await?;

        let import_result = import(path2, blobs.store(), &mut mp).await?;
        // wait for the endpoint to figure out its address before making a ticket
        let _ = router.endpoint().home_relay().initialized().await?;
        anyhow::Ok((router, import_result))
    };
    let (router, (temp_tag, size, collection)) = select! {
        x = setup => x?,
        _ = tokio::signal::ctrl_c() => {
            std::process::exit(130);
        }
    };
    let hash = *temp_tag.hash();

    // make a ticket
    let mut addr = router.endpoint().node_addr().await?;
    apply_options(&mut addr, args.ticket_type);
    let ticket = BlobTicket::new(addr, hash, BlobFormat::HashSeq)?;
    let entry_type = if path.is_file() { "file" } else { "directory" };
    println!(
        "imported {} {}, {}, hash {}",
        entry_type,
        path.display(),
        HumanBytes(size),
        print_hash(&hash, args.common.format)
    );
    if args.common.verbose > 1 {
        for (name, hash) in collection.iter() {
            println!("    {} {name}", print_hash(hash, args.common.format));
        }
    }
    println!("to get this data, use");
    println!("sendme receive {}", ticket);

    drop(temp_tag);

    // Wait for exit
    tokio::signal::ctrl_c().await?;

    println!("shutting down");
    tokio::time::timeout(Duration::from_secs(2), router.shutdown()).await??;
    tokio::fs::remove_dir_all(blobs_data_dir).await?;
    // drop everything that owns blobs to close the progress sender
    drop(router);
    // await progress completion so the progress bar is cleared
    progress.await.ok();

    Ok(())
}

fn make_download_progress() -> ProgressBar {
    let pb = ProgressBar::hidden();
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb.set_style(
        ProgressStyle::with_template(
            "{msg}{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} {binary_bytes_per_sec}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb
}

pub async fn show_download_progress(
    mp: MultiProgress,
    mut recv: mpsc::Receiver<u64>,
    local_size: u64,
    total_size: u64,
) -> anyhow::Result<()> {
    let op = mp.add(make_download_progress());
    op.set_message(format!("{} Connecting ...\n", style("[1/3]").bold().dim()));
    op.set_length(total_size);
    while let Some(offset) = recv.recv().await {
        op.set_position(local_size + offset);
    }
    op.finish_and_clear();
    Ok(())
}

fn show_get_error(e: anyhow::Error) -> anyhow::Error {
    if let Some(err) = e.downcast_ref::<DecodeError>() {
        match err {
            DecodeError::NotFound => {
                eprintln!("{}", style("send side no longer has a file").yellow())
            }
            DecodeError::LeafNotFound(_) | DecodeError::ParentNotFound(_) => eprintln!(
                "{}",
                style("send side no longer has part of a file").yellow()
            ),
            DecodeError::Io(err) => eprintln!(
                "{}",
                style(format!("generic network error: {}", err)).yellow()
            ),
            DecodeError::Read(err) => eprintln!(
                "{}",
                style(format!("error reading data from quinn: {}", err)).yellow()
            ),
            DecodeError::LeafHashMismatch(_) | DecodeError::ParentHashMismatch(_) => {
                eprintln!("{}", style("send side sent wrong data").red())
            }
        };
    } else if let Some(header_error) = e.downcast_ref::<AtBlobHeaderNextError>() {
        // TODO(iroh-bytes): get_to_db should have a concrete error type so you don't have to guess
        match header_error {
            AtBlobHeaderNextError::Io(err) => eprintln!(
                "{}",
                style(format!("generic network error: {}", err)).yellow()
            ),
            AtBlobHeaderNextError::Read(err) => eprintln!(
                "{}",
                style(format!("error reading data from quinn: {}", err)).yellow()
            ),
            AtBlobHeaderNextError::NotFound => {
                eprintln!("{}", style("send side no longer has a file").yellow())
            }
        };
    } else {
        eprintln!(
            "{}",
            style(format!("generic error: {:?}", e.root_cause())).red()
        );
    }
    e
}

async fn receive(args: ReceiveArgs) -> anyhow::Result<()> {
    let ticket = args.ticket;
    let addr = ticket.node_addr().clone();
    let secret_key = get_or_create_secret(args.common.verbose > 0)?;
    let mut builder = Endpoint::builder()
        .alpns(vec![])
        .secret_key(secret_key)
        .relay_mode(args.common.relay.into());

    if ticket.node_addr().relay_url.is_none() && ticket.node_addr().direct_addresses.is_empty() {
        builder = builder.add_discovery(|_| Some(DnsDiscovery::n0_dns()));
    }
    if let Some(addr) = args.common.magic_ipv4_addr {
        builder = builder.bind_addr_v4(addr);
    }
    if let Some(addr) = args.common.magic_ipv6_addr {
        builder = builder.bind_addr_v6(addr);
    }
    let endpoint = builder.bind().await?;
    let dir_name = format!(".sendme-get-{}", ticket.hash().to_hex());
    let iroh_data_dir = std::env::current_dir()?.join(dir_name);
    let db = blobs2::store::fs::FsStore::load(&iroh_data_dir).await?;
    let db2 = db.clone();
    trace!("load done!");
    let fut = async move {
        trace!("running");
        let mut mp: MultiProgress = MultiProgress::new();
        let draw_target = if args.common.no_progress {
            ProgressDrawTarget::hidden()
        } else {
            ProgressDrawTarget::stderr()
        };
        mp.set_draw_target(draw_target);
        let hash_and_format = ticket.hash_and_format();
        trace!("computing local");
        let local = db.download().local(hash_and_format).await?;
        trace!("local done");
        let (stats, total_files, payload_size) = if !local.is_complete() {
            trace!("{} not complete", hash_and_format.hash);
            let connect_progress = mp.add(ProgressBar::hidden());
            connect_progress.set_draw_target(ProgressDrawTarget::stderr());
            connect_progress.set_style(ProgressStyle::default_spinner());
            connect_progress.set_message(format!("connecting to {}", addr.node_id));
            let connection = endpoint.connect(addr, blobs2::protocol::ALPN).await?;
            connect_progress.finish_and_clear();
            // // trace!("request to complete sizes: {:#?}", local.complete_sizes());
            // db.download().execute_request(connection.clone(), local.complete_sizes(), None).await?;
            // db.dump().await?;
            // let local = db.download().local(hash_and_format).await?;
            // // println!("{:#?}", local);
            // trace!("{}", local.sizes().all(|x| x.is_some()));
            // let meta_size = local.sizes().take(2).map(Option::unwrap).sum::<u64>();
            // let total_files = local.children().unwrap() - 1;
            // let payload_size = local.sizes().skip(2).map(Option::unwrap).sum::<u64>();
            // let total_size = meta_size + payload_size;
            let (_hash_seq, sizes) =
                get_hash_seq_and_sizes(&connection, &hash_and_format.hash, 1024 * 1024 * 32, None)
                    .await
                    .map_err(show_get_error)?;
            let total_size = sizes.iter().map(|x| *x).sum::<u64>();
            let payload_size = sizes.iter().skip(2).map(|x| *x).sum::<u64>();
            let total_files = (sizes.len().saturating_sub(1)) as u64;
            eprintln!("got sizes");
            eprintln!(
                "getting collection {} {} files, {}",
                print_hash(&ticket.hash(), args.common.format),
                total_files,
                HumanBytes(payload_size)
            );
            // print the details of the collection only in verbose mode
            if args.common.verbose > 0 {
                eprintln!(
                    "getting {} blobs in total, {}",
                    total_files + 1,
                    HumanBytes(total_size)
                );
            }
            let (tx, rx) = mpsc::channel(32);
            // db.dump().await?;
            let local_size = local.local_bytes();
            // let missing = db.missing(hash_and_format).await?;
            // println!("{missing} missing blobs");
            // let request = GetRequest::new(hash_and_format.hash, missing);
            // let local_size = db.local_bytes(hash_and_format).await?;
            println!("local bytes: {}", local_size);
            // let get = db
            //     .download()
            //     .fetch(connection, hash_and_format, Some(tx.into()));
            let get = db
                .download()
                .execute_request(connection, local.missing(), Some(tx.into()));
            let task = tokio::spawn(show_download_progress(
                mp.clone(),
                rx,
                local_size,
                total_size,
            ));
            let stats = get.await.map_err(show_get_error)?;
            task.await.ok();
            let info = db.download().local(hash_and_format).await?;
            assert!(info.is_complete());
            (stats, total_files, payload_size)
        } else {
            println!("{} already complete", hash_and_format.hash);
            let total_files = local.children().unwrap() - 1;
            let payload_bytes = local.sizes().skip(2).map(Option::unwrap).sum::<u64>();
            (Stats::default(), total_files, payload_bytes)
        };
        let collection = Collection::load(hash_and_format.hash, db.as_ref()).await?;
        if args.common.verbose > 1 {
            for (name, hash) in collection.iter() {
                println!("    {} {name}", print_hash(hash, args.common.format));
            }
        }
        if let Some((name, _)) = collection.iter().next() {
            if let Some(first) = name.split('/').next() {
                println!("downloading to {}", first);
            }
        }
        println!("exporting");
        export(&db, collection, &mut mp).await?;
        anyhow::Ok((total_files, payload_size, stats))
    };
    let (total_files, payload_size, stats) = select! {
        x = fut => x?,
        _ = tokio::signal::ctrl_c() => {
            db2.shutdown().await?;
            std::process::exit(130);
        }
    };
    tokio::fs::remove_dir_all(iroh_data_dir).await?;
    if args.common.verbose > 0 {
        println!(
            "downloaded {} files, {}. took {} ({}/s)",
            total_files,
            HumanBytes(payload_size),
            HumanDuration(stats.elapsed),
            HumanBytes((stats.total_bytes_read() as f64 / stats.elapsed.as_secs_f64()) as u64),
        );
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(cause) => {
            if let Some(text) = cause.get(ContextKind::InvalidSubcommand) {
                eprintln!("{} \"{}\"\n", ErrorKind::InvalidSubcommand, text);
                eprintln!("Available subcommands are");
                for cmd in Args::command().get_subcommands() {
                    eprintln!("    {}", style(cmd.get_name()).bold());
                }
                std::process::exit(1);
            } else {
                cause.exit();
            }
        }
    };
    let res = match args.command {
        Commands::Send(args) => send(args).await,
        Commands::Receive(args) => receive(args).await,
    };
    if let Err(e) = &res {
        eprintln!("{e}");
    }
    match res {
        Ok(()) => std::process::exit(0),
        Err(_) => std::process::exit(1),
    }
}
