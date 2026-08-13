use crate::config::RuntimeConfig;
use crate::error::WebBootstrapError;
use crate::plugin_api::security::PluginSecurity;
use crate::plugin_api::{PluginBackend, PluginBackendAdapter};
use crate::{AppState, build_app_state, build_router};
use ora_plugin_manager::{
    DirectoryRuntimeAssetSource, DiscoveryRootId, ManagerLease, PluginManagementService,
    PluginManagerConfig, PluginRuntimeAssets, PluginRuntimeHub, ProcessTreeGenerationLauncher,
    RuntimeAssetStore, SystemAuthorityClock, UnavailableLaunchValueResolver,
};
#[cfg(windows)]
use ora_process::WindowsJobProcessTreeSpawner;
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Trusted bootstrap values transferred to the main Tauri window through in-process IPC.
pub struct BackendBootstrapCredentials {
    endpoint: SocketAddr,
    bearer: String,
}

impl BackendBootstrapCredentials {
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    pub fn bearer(&self) -> &str {
        &self.bearer
    }
}

/// Explicit application resource and WebView-origin policy for authenticated plugin routes.
pub struct PluginBackendOptions {
    runtime_resources: PathBuf,
    allowed_origins: Vec<String>,
    discovery_roots: BTreeMap<DiscoveryRootId, PathBuf>,
    expose_authenticated_routes: bool,
}

impl PluginBackendOptions {
    pub fn new(runtime_resources: impl Into<PathBuf>, allowed_origins: Vec<String>) -> Self {
        Self {
            runtime_resources: runtime_resources.into(),
            allowed_origins,
            discovery_roots: BTreeMap::new(),
            expose_authenticated_routes: true,
        }
    }

    pub fn with_discovery_root(mut self, id: DiscoveryRootId, path: PathBuf) -> Self {
        self.discovery_roots.insert(id, path);
        self
    }

    /// Keeps the manager/runtime composition active while omitting routes without trusted IPC.
    pub fn without_plugin_routes(mut self) -> Self {
        self.expose_authenticated_routes = false;
        self.allowed_origins.clear();
        self
    }
}

/// Owns the loopback listener, authenticated plugin facade, runtime trees, state writer, and lease.
pub struct BackendRuntime {
    app_state: AppState,
    endpoint: SocketAddr,
    credentials: Option<BackendBootstrapCredentials>,
    plugin_backend: Arc<dyn PluginBackend>,
    shutdown: CancellationToken,
    server: JoinHandle<Result<(), std::io::Error>>,
}

const BACKEND_SHUTDOWN_HARD_DEADLINE: Duration = Duration::from_secs(60);

impl BackendRuntime {
    /// Boots every backend dependency before readiness and binds production to `127.0.0.1:0`.
    #[cfg(windows)]
    pub async fn start(
        runtime_config: &RuntimeConfig,
        options: PluginBackendOptions,
    ) -> Result<Self, WebBootstrapError> {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(WebBootstrapError::Bind)?;
        let endpoint = listener.local_addr().map_err(WebBootstrapError::Bind)?;
        let security = if options.expose_authenticated_routes {
            let mut bearer = [0u8; 32];
            getrandom::fill(&mut bearer).map_err(|error| WebBootstrapError::PluginSecurity {
                message: error.to_string(),
            })?;
            Some(
                PluginSecurity::new(bearer, endpoint.to_string(), options.allowed_origins)
                    .map_err(|message| WebBootstrapError::PluginSecurity { message })?,
            )
        } else {
            None
        };

        let data_dir = runtime_config
            .database()
            .path()
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let manager_config = PluginManagerConfig::new(data_dir);
        let lease = Arc::new(
            ManagerLease::acquire(&manager_config).map_err(WebBootstrapError::PluginBootstrap)?,
        );
        let source = Arc::new(DirectoryRuntimeAssetSource::new(options.runtime_resources));
        let asset_store = RuntimeAssetStore::new(manager_config.plugin_runtime_dir(), source);
        let asset_lease = asset_store
            .prepare()
            .await
            .map_err(WebBootstrapError::PluginBootstrap)?;
        let mut assets = PluginRuntimeAssets::from_runtime_lease(asset_lease)
            .await
            .map_err(WebBootstrapError::PluginBootstrap)?;
        for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            let value = std::env::var_os(key).ok_or_else(|| {
                WebBootstrapError::PluginBootstrap(
                    ora_plugin_manager::PluginError::PluginRuntimeUnavailable,
                )
            })?;
            assets = assets.with_environment(key, value);
        }
        let launcher = ProcessTreeGenerationLauncher::new(
            WindowsJobProcessTreeSpawner::new()
                .with_tree_cleanup_timeout(manager_config.deadlines.tree_cleanup),
        );
        let runtime = PluginRuntimeHub::new(
            manager_config.clone(),
            assets,
            launcher,
            UnavailableLaunchValueResolver,
        );
        let management = Arc::new(
            PluginManagementService::bootstrap_with_lease(
                manager_config,
                SystemAuthorityClock::new(),
                runtime.clone(),
                options.discovery_roots,
                lease,
            )
            .await
            .map_err(WebBootstrapError::PluginBootstrap)?,
        );
        runtime
            .bind(
                Arc::clone(&management),
                Arc::new(management.runtime_event_sink()),
            )
            .map_err(WebBootstrapError::PluginBootstrap)?;
        let plugin_backend: Arc<dyn PluginBackend> = Arc::new(
            PluginBackendAdapter::new(management, runtime)
                .map_err(WebBootstrapError::PluginBootstrap)?,
        );
        let mut app_state = build_app_state(runtime_config)?;
        if let Some(security) = &security {
            app_state =
                app_state.with_plugin_backend(Arc::clone(&plugin_backend), security.clone());
        }
        let router = build_router(app_state.clone());
        app_state.mark_ready();

        let shutdown = CancellationToken::new();
        let server_shutdown = shutdown.clone();
        let unexpected_exit_guard = shutdown.clone();
        let server = tokio::spawn(async move {
            let result = axum::serve(listener, router)
                .with_graceful_shutdown(server_shutdown.cancelled_owned())
                .await;
            if !unexpected_exit_guard.is_cancelled() {
                std::process::abort();
            }
            result
        });
        let credentials = security.map(|security| BackendBootstrapCredentials {
            endpoint,
            bearer: security.bearer_hex(),
        });
        Ok(Self {
            app_state,
            endpoint,
            credentials,
            plugin_backend,
            shutdown,
            server,
        })
    }

    #[cfg(not(windows))]
    pub async fn start(
        _runtime_config: &RuntimeConfig,
        _options: PluginBackendOptions,
    ) -> Result<Self, WebBootstrapError> {
        Err(WebBootstrapError::PluginBootstrap(
            ora_plugin_manager::PluginError::TreeKillUnavailable {
                plugin_id: ora_plugin_protocol::PluginId::parse("ora.backend").map_err(
                    |error| WebBootstrapError::PluginSecurity {
                        message: error.to_string(),
                    },
                )?,
            },
        ))
    }

    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    pub fn credentials(&self) -> Option<&BackendBootstrapCredentials> {
        self.credentials.as_ref()
    }

    /// Converts a trusted native-picker result into a session-bound opaque selection.
    pub fn register_native_selection(
        &self,
        path: &Path,
    ) -> Result<ora_contracts::NativePluginSelectionResponse, WebBootstrapError> {
        self.plugin_backend
            .register_native_selection(path)
            .map_err(WebBootstrapError::PluginBootstrap)
    }

    /// Mints one removal capability only after the Tauri composition root confirms user intent.
    pub fn authorize_all_owner_data_removal(
        &self,
        plugin_id: ora_plugin_protocol::PluginId,
    ) -> Result<ora_contracts::DataRemovalConfirmationResponse, WebBootstrapError> {
        self.plugin_backend
            .authorize_all_owner_data_removal(plugin_id)
            .map_err(WebBootstrapError::PluginBootstrap)
    }

    /// Drains accepted HTTP work, then stops plugin trees before releasing the ManagerLease owner.
    pub async fn shutdown(self) -> Result<(), WebBootstrapError> {
        let BackendRuntime {
            app_state,
            plugin_backend,
            shutdown,
            server,
            ..
        } = self;
        app_state.mark_unready();
        plugin_backend.close_admission();
        shutdown.cancel();
        app_state.cancel_plugin_invocations().await;
        let graceful = async move {
            let (plugin_result, server_result) = tokio::join!(plugin_backend.shutdown(), server);
            plugin_result.map_err(WebBootstrapError::PluginBootstrap)?;
            server_result
                .map_err(WebBootstrapError::BackendTask)?
                .map_err(WebBootstrapError::Serve)
        };
        match tokio::time::timeout(BACKEND_SHUTDOWN_HARD_DEADLINE, graceful).await {
            Ok(result) => result,
            Err(_) => std::process::abort(),
        }
    }
}
