pub mod core;
pub mod setup;
use core::{Manager, NetworkCheck, Snapshot, Workspace};
use setup::{SetupManager, SetupStatus};
use std::{path::PathBuf, sync::Arc};
use tauri::{Manager as _, State};

#[tauri::command]
async fn setup_status(setup: State<'_, Arc<SetupManager>>) -> Result<SetupStatus, String> {
    Ok(setup.status().await)
}
#[tauri::command]
async fn run_setup(action: String, setup: State<'_, Arc<SetupManager>>) -> Result<(), String> {
    setup.run(&action).await
}
#[tauri::command]
async fn dismiss_setup(setup: State<'_, Arc<SetupManager>>) -> Result<(), String> {
    setup.dismiss().await
}
#[tauri::command]
async fn open_setup_help(topic: String) -> Result<(), String> {
    setup::open_help(&topic).await
}

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
async fn check_saved_credentials(manager: State<'_, Arc<Manager>>) -> Result<(), String> {
    manager.check_saved_credentials().await
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
    setup: State<'_, Arc<SetupManager>>,
) -> Result<(), String> {
    if setup.is_busy().await {
        return Err("Wait for the current setup step to finish before closing RegionBox.".into());
    }
    if manager.is_busy() {
        return Err("Wait for the current connection check or workspace action to finish.".into());
    }
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
            app.manage(SetupManager::new(root.clone()).map_err(std::io::Error::other)?);
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
            check_saved_credentials,
            verify_workspace,
            workspace_logs,
            quit_app,
            setup_status,
            run_setup,
            dismiss_setup,
            open_setup_help
        ])
        .run(tauri::generate_context!())
        .expect("RegionBox could not start");
}
