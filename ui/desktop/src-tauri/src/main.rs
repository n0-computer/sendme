// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use tauri::Manager;

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
            .finish()
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
