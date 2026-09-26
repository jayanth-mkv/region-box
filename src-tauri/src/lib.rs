pub mod core;
use core::{Manager, NetworkCheck, Snapshot, Workspace};
use std::{path::PathBuf, sync::Arc};
use tauri::{Manager as _, State};

#[tauri::command]
async fn snapshot(manager: State<'_, Arc<Manager>>) -> Result<Snapshot, String> {
    Ok(manager.snapshot().await)
}
#[tauri::command]
async fn start_workspace(id: String, manager: State<'_, Arc<Manager>>) -> Result<(), String> {
    manager.start(&id).await
}
#[tauri::command]
async fn stop_workspace(id: String, manager: State<'_, Arc<Manager>>) -> Result<(), String> {
    manager.stop(&id).await
}
#[tauri::command]
async fn stop_all(manager: State<'_, Arc<Manager>>) -> Result<(), String> {
    manager.stop_all().await
}
#[tauri::command]
async fn create_workspace(
    name: String,
    country: String,
    manager: State<'_, Arc<Manager>>,
) -> Result<Workspace, String> {
    manager.create(name, country).await
}
#[tauri::command]
async fn save_credentials(
    user: String,
    password: String,
    manager: State<'_, Arc<Manager>>,
) -> Result<(), String> {
    manager.save_credentials(user, password).await
}
#[tauri::command]
async fn verify_workspace(
    id: String,
    manager: State<'_, Arc<Manager>>,
) -> Result<NetworkCheck, String> {
    manager.verify(&id).await
}
#[tauri::command]
async fn workspace_logs(id: String, manager: State<'_, Arc<Manager>>) -> Result<String, String> {
    manager.logs(&id).await
}
#[tauri::command]
async fn quit_app(
    stop: bool,
    app: tauri::AppHandle,
    manager: State<'_, Arc<Manager>>,
) -> Result<(), String> {
    if stop {
        manager.stop_all().await?;
    }
    app.exit(0);
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let root = std::env::var_os("REGIONBOX_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(app.path().app_local_data_dir()?);
            let credentials = std::env::var_os("REGIONBOX_ENV_FILE")
                .map(PathBuf::from)
                .or_else(|| {
                    if cfg!(debug_assertions) {
                        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.env"))
                    } else {
                        None
                    }
                });
            app.manage(Manager::new(root, credentials).map_err(std::io::Error::other)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            start_workspace,
            stop_workspace,
            stop_all,
            create_workspace,
            save_credentials,
            verify_workspace,
            workspace_logs,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("RegionBox could not start");
}
