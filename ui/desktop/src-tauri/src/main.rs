// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;
use iroh::bytes::format::collection::Collection;

use anyhow::{anyhow, Context, Result};
use tauri::Manager;
use walkdir::WalkDir;

// this example uses a persistend iroh node stored in the application data directory
type IrohNode = iroh::node::Node<iroh::bytes::store::fs::Store>;

// setup an iroh node
async fn setup<R: tauri::Runtime>(handle: tauri::AppHandle<R>) -> Result<()> {
    // get the application data root, join with "iroh_data" to get the data root for the iroh node
    let data_root = handle
        .path_resolver()
        .app_data_dir()
        .ok_or_else(|| anyhow!("can't get application data directory"))?
        .join("iroh_data");

    // create the iroh node
    let node = iroh::node::Node::persistent(data_root)
        .await?
        .spawn()
        .await?;
    handle.manage(AppState::new(node));

    Ok(())
}

struct AppState {
    iroh: IrohNode,
}

impl AppState {
    fn new(iroh: IrohNode) -> Self {
        AppState { iroh }
    }

    fn iroh(&self) -> iroh::client::mem::Iroh {
        self.iroh.client().clone()
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();
            #[cfg(debug_assertions)] // only include this code on debug builds
            {
                let window = app.get_window("main").unwrap();
                window.open_devtools();
            }

            tauri::async_runtime::spawn(async move {
                println!("starting backend...");
                if let Err(err) = setup(handle).await {
                    eprintln!("failed: {:?}", err);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![send])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn send(files: Vec<String>, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let client = state.iroh();
    let mut collection = BTreeMap::new();
    let mut tags_to_delete = Vec::new();
    for file in files {
        let path: PathBuf = file.clone().into();
        client.blobs.add_from_path(path, in_place, tag, wrap)
        let res = client
            .blobs
            .add_from_path(
                path,
                false,
                iroh::rpc_protocol::SetTagOption::Auto,
                iroh::rpc_protocol::WrapOption::NoWrap,
            )
            .await
            .map_err(|e| e.to_string())?
            .await
            .map_err(|e| e.to_string())?;
        tags_to_delete.push(res.tag);
        collection.insert(file, res.hash);
    }
    let collection = iroh::bytes::format::collection::Collection::from_iter(collection.into_iter());
    let (hash, _) = client
        .blobs
        .create_collection(
            collection,
            iroh::rpc_protocol::SetTagOption::Auto,
            tags_to_delete,
        )
        .await
        .map_err(|e| e.to_string())?;
    let ticket = client
        .blobs
        .share(
            hash,
            iroh::bytes::BlobFormat::HashSeq,
            iroh::client::ShareTicketOptions::RelayAndAddresses,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(ticket.to_string())
}

// async fn receive(ticket: String, state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
//     let client = state.iroh();
//     let ticket = iroh::base::ticket::BlobTicket::from_str(&ticket).map_err(|e| e.to_string())?;

//     if !ticket.recursive() {
//         return Err("ticket is not for a collection".to_string());
//     }

//     let collection = client
//         .blobs
//         .get_collection(ticket)
//         .await
//         .map_err(|e| e.to_string())?;
//     let mut files = Vec::new();
//     for (path, hash) in collection.iter() {
//         let file = client
//             .blobs
//             .get_to_path(hash, path.clone().into())
//             .await
//             .map_err(|e| e.to_string())?;
//         files.push(file.to_string_lossy().to_string());
//     }
//     Ok(files)
// }


/// Import from a file or directory into the database.
///
/// The returned tag always refers to a collection. If the input is a file, this
/// is a collection with a single blob, named like the file.
///
/// If the input is a directory, the collection contains all the files in the
/// directory.
async fn import(
    path: PathBuf,
    db: impl iroh::bytes::store::Store,
) -> anyhow::Result<(iroh::bytes::TempTag, u64, iroh::bytes::format::collection::Collection)> {
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
    let names_and_tags = futures::stream::iter(data_sources)
        .map(|(name, path)| {
            let db = db.clone();
            // let progress = progress.clone();
            async move {
                let (temp_tag, file_size) = db
                    .import_file(path, iroh::bytes::store::ImportMode::TryReference, iroh::bytes::BlobFormat::Raw, progress)
                    .await?;
                anyhow::Ok((name, temp_tag, file_size))
            }
        })
        .buffer_unordered(num_cpus::get())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;

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