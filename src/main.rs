//! Command line arguments.
use anyhow::Context;
use clap::{
    error::{ContextKind, ErrorKind},
    CommandFactory, Parser, Subcommand,
};
use console::style;
use futures_buffered::BufferedStreamExt;
use futures_lite::{future::Boxed, Stream, StreamExt};
use indicatif::{
    HumanBytes, HumanDuration, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle,
};
use iroh_base::ticket::BlobTicket;
use iroh_blobs::{
    format::collection::Collection,
    get::{
        db::DownloadProgress,
        fsm::{AtBlobHeaderNextError, DecodeError},
        request::get_hash_seq_and_sizes,
    },
    provider::{self, handle_connection, EventSender},
    store::{ExportMode, ImportMode, ImportProgress},
    BlobFormat, Hash, HashAndFormat, TempTag,
};
use iroh_net::{key::SecretKey, Endpoint};
use rand::Rng;
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter},
    path::{Component, Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tokio_util::task::LocalPoolHandle;
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
    Receive(ReceiveArgs),
}

#[derive(Parser, Debug)]
pub struct CommonArgs {
    /// The port for the magic socket to listen on.
    ///
    /// Defauls to a random free port, but it can be useful to specify a fixed
    /// port, e.g. to configure a firewall rule.
    #[clap(long, default_value_t = 0)]
    pub magic_port: u16,

    #[clap(long, default_value_t = Format::Hex)]
    pub format: Format,

    #[clap(short = 'v', long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}
#[derive(Parser, Debug)]
pub struct SendArgs {
    /// Path to the file or directory to send.
    ///
    /// The last component of the path will be used as the name of the data
    /// being shared.
    pub path: PathBuf,

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

/// Get the secret key or generate a new one.
///
/// Print the secret key to stderr if it was generated, so the user can save it.
fn get_or_create_secret(print: bool) -> anyhow::Result<SecretKey> {
    match std::env::var("IROH_SECRET") {
        Ok(secret) => SecretKey::from_str(&secret).context("invalid secret"),
        Err(_) => {
            let key = SecretKey::generate();
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

pub async fn show_ingest_progress(
    mut stream: impl Stream<Item = ImportProgress> + Unpin,
) -> anyhow::Result<()> {
    let mp = MultiProgress::new();
    mp.set_draw_target(ProgressDrawTarget::stderr());
    let op = mp.add(ProgressBar::hidden());
    op.set_style(
        ProgressStyle::default_spinner().template("{spinner:.green} [{elapsed_precise}] {msg}")?,
    );
    // op.set_message(format!("{} Ingesting ...\n", style("[1/2]").bold().dim()));
    // op.set_length(total_files);
    let mut names = BTreeMap::new();
    let mut sizes = BTreeMap::new();
    let mut pbs = BTreeMap::new();
    while let Some(event) = stream.next().await {
        match event {
            ImportProgress::Found { id, name } => {
                names.insert(id, name);
            }
            ImportProgress::Size { id, size } => {
                sizes.insert(id, size);
                let total_size = sizes.values().sum::<u64>();
                op.set_message(format!(
                    "{} Ingesting {} files, {}\n",
                    style("[1/2]").bold().dim(),
                    sizes.len(),
                    HumanBytes(total_size)
                ));
                let name = names.get(&id).cloned().unwrap_or_default();
                let pb = mp.add(ProgressBar::hidden());
                pb.set_style(ProgressStyle::with_template(
                    "{msg}{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
                )?.progress_chars("#>-"));
                pb.set_message(format!("{} {}", style("[2/2]").bold().dim(), name));
                pb.set_length(size);
                pbs.insert(id, pb);
            }
            ImportProgress::OutboardProgress { id, offset } => {
                if let Some(pb) = pbs.get(&id) {
                    pb.set_position(offset);
                }
            }
            ImportProgress::OutboardDone { id, .. } => {
                // you are not guaranteed to get any OutboardProgress
                if let Some(pb) = pbs.remove(&id) {
                    pb.finish_and_clear();
                }
            }
            ImportProgress::CopyProgress { .. } => {
                // we are not copying anything
            }
        }
    }
    op.finish_and_clear();
    Ok(())
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
    db: impl iroh_blobs::store::Store,
) -> anyhow::Result<(TempTag, u64, Collection)> {
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
    let (send, recv) = flume::bounded(32);
    let progress = iroh_blobs::util::progress::FlumeProgressSender::new(send);
    let show_progress = tokio::spawn(show_ingest_progress(recv.into_stream()));
    // import all the files, using num_cpus workers, return names and temp tags
    let mut names_and_tags = futures_lite::stream::iter(data_sources)
        .map(|(name, path)| {
            let db = db.clone();
            let progress = progress.clone();
            async move {
                let (temp_tag, file_size) = db
                    .import_file(path, ImportMode::TryReference, BlobFormat::Raw, progress)
                    .await?;
                anyhow::Ok((name, temp_tag, file_size))
            }
        })
        .buffered_unordered(num_cpus::get())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;
    drop(progress);
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
    show_progress.await??;
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

async fn export(db: impl iroh_blobs::store::Store, collection: Collection) -> anyhow::Result<()> {
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
        db.export(
            *hash,
            target,
            ExportMode::TryReference,
            Box::new(move |_position| Ok(())),
        )
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SendStatus {
    /// the multiprogress bar
    mp: MultiProgress,
}

impl SendStatus {
    fn new() -> Self {
        let mp = MultiProgress::new();
        mp.set_draw_target(ProgressDrawTarget::stderr());
        Self { mp }
    }

    fn new_client(&self) -> ClientStatus {
        let current = self.mp.add(ProgressBar::hidden());
        current.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        current.enable_steady_tick(Duration::from_millis(100));
        current.set_message("waiting for requests");
        ClientStatus {
            current: current.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct ClientStatus {
    current: Arc<ProgressBar>,
}

impl Drop for ClientStatus {
    fn drop(&mut self) {
        if Arc::strong_count(&self.current) == 1 {
            self.current.finish_and_clear();
        }
    }
}

impl EventSender for ClientStatus {
    fn send(&self, event: iroh_blobs::provider::Event) -> Boxed<()> {
        tracing::info!("{:?}", event);
        let msg = match event {
            provider::Event::ClientConnected { connection_id } => {
                Some(format!("{} got connection", connection_id))
            }
            provider::Event::TransferBlobCompleted {
                connection_id,
                hash,
                index,
                size,
                ..
            } => Some(format!(
                "{} transfer blob completed {} {} {}",
                connection_id,
                hash,
                index,
                HumanBytes(size)
            )),
            provider::Event::TransferCompleted {
                connection_id,
                stats,
                ..
            } => Some(format!(
                "{} transfer completed {} {}",
                connection_id,
                stats.send.write_bytes.size,
                HumanDuration(stats.send.write_bytes.stats.duration)
            )),
            provider::Event::TransferAborted { connection_id, .. } => {
                Some(format!("{} transfer completed", connection_id))
            }
            _ => None,
        };
        if let Some(msg) = msg {
            self.current.set_message(msg);
        }
        Box::pin(std::future::ready(()))
    }
}

async fn send(args: SendArgs) -> anyhow::Result<()> {
    let secret_key = get_or_create_secret(args.common.verbose > 0)?;
    // create a magicsocket endpoint
    let endpoint_fut = Endpoint::builder()
        .alpns(vec![iroh_blobs::protocol::ALPN.to_vec()])
        .secret_key(secret_key)
        .bind(args.common.magic_port);
    // use a flat store - todo: use a partial in mem store instead
    let suffix = rand::thread_rng().gen::<[u8; 16]>();
    let iroh_data_dir =
        std::env::current_dir()?.join(format!(".sendme-send-{}", hex::encode(suffix)));
    if iroh_data_dir.exists() {
        println!("can not share twice from the same directory");
        std::process::exit(1);
    }
    let iroh_data_dir_2 = iroh_data_dir.clone();
    let _control_c = tokio::spawn(async move {
        tokio::signal::ctrl_c().await?;
        std::fs::remove_dir_all(iroh_data_dir_2)?;
        std::process::exit(1);
        #[allow(unreachable_code)]
        anyhow::Ok(())
    });
    std::fs::create_dir_all(&iroh_data_dir)?;
    let db = iroh_blobs::store::fs::Store::load(&iroh_data_dir).await?;
    let path = args.path;
    let (temp_tag, size, collection) = import(path.clone(), db.clone()).await?;
    let hash = *temp_tag.hash();
    // wait for the endpoint to be ready
    let endpoint = endpoint_fut.await?;
    // wait for the endpoint to figure out its address before making a ticket
    while endpoint.my_relay().is_none() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // make a ticket
    let addr = endpoint.my_addr().await?;
    let ticket = BlobTicket::new(addr, hash, BlobFormat::HashSeq)?;
    let entry_type = if path.is_file() { "file" } else { "directory" };
    println!(
        "imported {} {}, {}, hash {}",
        entry_type,
        path.display(),
        HumanBytes(size),
        print_hash(&hash, args.common.format)
    );
    if args.common.verbose > 0 {
        for (name, hash) in collection.iter() {
            println!("    {} {name}", print_hash(hash, args.common.format));
        }
    }
    println!("to get this data, use");
    println!("sendme receive {}", ticket);
    let ps = SendStatus::new();
    let rt = LocalPoolHandle::new(1);
    loop {
        let Some(connecting) = endpoint.accept().await else {
            tracing::info!("no more incoming connections, exiting");
            break;
        };
        let db = db.clone();
        let rt = rt.clone();
        let ps = ps.clone();
        let connection = connecting.await?;
        tokio::spawn(handle_connection(connection, db, ps.new_client(), rt));
    }
    drop(temp_tag);
    std::fs::remove_dir_all(iroh_data_dir)?;
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
    mut stream: impl Stream<Item = DownloadProgress> + Unpin,
    total_size: u64,
) -> anyhow::Result<()> {
    let mp = MultiProgress::new();
    mp.set_draw_target(ProgressDrawTarget::stderr());
    let op = mp.add(make_download_progress());
    op.set_message(format!("{} Connecting ...\n", style("[1/3]").bold().dim()));
    let mut total_done = 0;
    let mut sizes = BTreeMap::new();
    while let Some(x) = stream.next().await {
        match x {
            DownloadProgress::Connected => {
                op.set_message(format!("{} Requesting ...\n", style("[2/3]").bold().dim()));
            }
            DownloadProgress::FoundHashSeq { children, .. } => {
                op.set_message(format!(
                    "{} Downloading {} blob(s)\n",
                    style("[3/3]").bold().dim(),
                    children + 1,
                ));
                op.set_length(total_size);
                op.reset();
            }
            DownloadProgress::Found { id, size, .. } => {
                sizes.insert(id, size);
            }
            DownloadProgress::Progress { offset, .. } => {
                op.set_position(total_done + offset);
            }
            DownloadProgress::Done { id } => {
                total_done += sizes.remove(&id).unwrap_or_default();
            }
            DownloadProgress::AllDone(stats) => {
                op.finish_and_clear();
                eprintln!(
                    "Transferred {} in {}, {}/s",
                    HumanBytes(stats.bytes_read),
                    HumanDuration(stats.elapsed),
                    HumanBytes((stats.bytes_read as f64 / stats.elapsed.as_secs_f64()) as u64)
                );
                break;
            }
            DownloadProgress::Abort(e) => {
                anyhow::bail!("download aborted: {:?}", e);
            }
            _ => {}
        }
    }
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
    let endpoint = Endpoint::builder()
        .alpns(vec![])
        .secret_key(secret_key)
        .bind(args.common.magic_port)
        .await?;
    let dir_name = format!(".sendme-get-{}", ticket.hash().to_hex());
    let iroh_data_dir = std::env::current_dir()?.join(dir_name);
    let db = iroh_blobs::store::fs::Store::load(&iroh_data_dir).await?;
    let mp = MultiProgress::new();
    let connect_progress = mp.add(ProgressBar::hidden());
    connect_progress.set_draw_target(ProgressDrawTarget::stderr());
    connect_progress.set_style(ProgressStyle::default_spinner());
    connect_progress.set_message(format!("connecting to {}", addr.node_id));
    let connection = endpoint.connect(addr, iroh_blobs::protocol::ALPN).await?;
    let hash_and_format = HashAndFormat {
        hash: ticket.hash(),
        format: ticket.format(),
    };
    connect_progress.finish_and_clear();
    let (send, recv) = flume::bounded(32);
    let progress = iroh_blobs::util::progress::FlumeProgressSender::new(send);
    let (_hash_seq, sizes) =
        get_hash_seq_and_sizes(&connection, &hash_and_format.hash, 1024 * 1024 * 32)
            .await
            .map_err(show_get_error)?;
    let total_size = sizes.iter().sum::<u64>();
    let total_files = sizes.len().saturating_sub(1);
    let payload_size = sizes.iter().skip(1).sum::<u64>();
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
            sizes.len(),
            HumanBytes(total_size)
        );
    }
    let _task = tokio::spawn(show_download_progress(recv.into_stream(), total_size));
    let get_conn = || async move { Ok(connection) };
    let stats = iroh_blobs::get::db::get_to_db(&db, get_conn, &hash_and_format, progress)
        .await
        .map_err(|e| show_get_error(anyhow::anyhow!(e)))?;
    let collection = Collection::load_db(&db, &hash_and_format.hash).await?;
    if args.common.verbose > 0 {
        for (name, hash) in collection.iter() {
            println!("    {} {name}", print_hash(hash, args.common.format));
        }
    }
    if let Some((name, _)) = collection.iter().next() {
        if let Some(first) = name.split('/').next() {
            println!("downloading to: {};", first);
        }
    }
    export(db, collection).await?;
    std::fs::remove_dir_all(iroh_data_dir)?;
    if args.common.verbose > 0 {
        println!(
            "downloaded {} files, {}. took {} ({}/s)",
            total_files,
            HumanBytes(payload_size),
            HumanDuration(stats.elapsed),
            HumanBytes((stats.bytes_read as f64 / stats.elapsed.as_secs_f64()) as u64),
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
    match res {
        Ok(()) => std::process::exit(0),
        Err(_) => std::process::exit(1),
    }
}
