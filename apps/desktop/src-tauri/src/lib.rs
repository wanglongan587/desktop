use ora_contracts::{DataRemovalConfirmationResponse, NativePluginSelectionResponse};
use ora_domain::ProjectId;
use ora_plugin_protocol::PluginId;
use ora_web_server::config::RuntimeConfig;
use ora_web_server::{BackendRuntime, PluginBackendOptions};
use serde::Serialize;
use std::sync::{Mutex, PoisonError};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

/// Creates a shared domain identifier during startup so the desktop shell stays compiled against the canonical domain crate.
fn bootstrap_project_id() -> ProjectId {
    ProjectId::new("desktop-bootstrap")
}

struct DesktopBackendState {
    runtime: Mutex<Option<BackendRuntime>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendBootstrapPayload {
    endpoint: String,
    bearer: String,
}

/// Returns backend authority only to the configured main Workbench window after readiness.
#[tauri::command]
fn plugin_backend_bootstrap(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopBackendState>,
) -> Result<BackendBootstrapPayload, String> {
    require_main_window(&window)?;
    let guard = state.runtime.lock().unwrap_or_else(PoisonError::into_inner);
    let runtime = guard
        .as_ref()
        .ok_or_else(|| "backend is not ready".to_owned())?;
    let credentials = runtime
        .credentials()
        .ok_or_else(|| "authenticated plugin routes are not available".to_owned())?;
    Ok(BackendBootstrapPayload {
        endpoint: format!("http://{}", credentials.endpoint()),
        bearer: credentials.bearer().to_owned(),
    })
}

/// Opens the operating-system folder picker and returns only an opaque selection capability.
#[tauri::command]
async fn plugin_pick_candidate(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopBackendState>,
) -> Result<NativePluginSelectionResponse, String> {
    require_main_window(&window)?;
    let Some(selection) = window.dialog().file().blocking_pick_folder() else {
        return Ok(NativePluginSelectionResponse { selection: None });
    };
    let path = selection
        .into_path()
        .map_err(|_| "selected location is not a local filesystem path".to_owned())?;
    let guard = state.runtime.lock().unwrap_or_else(PoisonError::into_inner);
    let runtime = guard
        .as_ref()
        .ok_or_else(|| "backend is not ready".to_owned())?;
    runtime
        .register_native_selection(&path)
        .map_err(|error| error.to_string())
}

/// Uses a native destructive prompt before minting a one-time all-owner data removal capability.
#[tauri::command]
async fn plugin_confirm_all_owner_data_removal(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopBackendState>,
    plugin_id: String,
) -> Result<Option<DataRemovalConfirmationResponse>, String> {
    require_main_window(&window)?;
    let plugin_id = PluginId::parse(plugin_id).map_err(|_| "plugin id is invalid".to_owned())?;
    let confirmed = window
        .dialog()
        .message(format!(
            "Remove all saved data for {} across every installed content owner?",
            plugin_id
        ))
        .title("Remove plugin data")
        .buttons(MessageDialogButtons::YesNo)
        .blocking_show();
    if !confirmed {
        return Ok(None);
    }
    let guard = state.runtime.lock().unwrap_or_else(PoisonError::into_inner);
    let runtime = guard
        .as_ref()
        .ok_or_else(|| "backend is not ready".to_owned())?;
    runtime
        .authorize_all_owner_data_removal(plugin_id)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn require_main_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("backend authority is restricted to the main Workbench window".to_owned());
    }
    Ok(())
}

/// Starts the Tauri application and wires in development-only logging.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(DesktopBackendState {
            runtime: Mutex::new(None),
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            plugin_backend_bootstrap,
            plugin_pick_candidate,
            plugin_confirm_all_owner_data_removal
        ]);
    // The embedded driver is compiled only into the dedicated E2E binary, so production builds
    // never expose an automation server or its additional attack surface.
    #[cfg(feature = "desktop-e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    let app = builder
        .setup(|app| {
            let _ = bootstrap_project_id();
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            // Renderer/window smoke tests do not need the separately covered plugin runtime.
            // Keeping that dependency out makes the native window gate deterministic and fast.
            if cfg!(feature = "desktop-e2e") {
                return Ok(());
            }
            let runtime_config = RuntimeConfig::from_env()?;
            let resources = app.path().resource_dir()?.join("plugin-runtime");
            let options = PluginBackendOptions::new(
                resources,
                vec![
                    "http://tauri.localhost".to_owned(),
                    "tauri://localhost".to_owned(),
                    "http://127.0.0.1:5173".to_owned(),
                ],
            );
            let backend =
                tauri::async_runtime::block_on(BackendRuntime::start(&runtime_config, options))?;
            let state = app.state::<DesktopBackendState>();
            *state.runtime.lock().unwrap_or_else(PoisonError::into_inner) = Some(backend);
            Ok(())
        })
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("error while building Tauri application: {error}"));
    let exit_code = app.run_return(|handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event
            && let Some(state) = handle.try_state::<DesktopBackendState>()
        {
            let runtime = state
                .runtime
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(runtime) = runtime {
                let _ = tauri::async_runtime::block_on(runtime.shutdown());
            }
        }
    });
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
