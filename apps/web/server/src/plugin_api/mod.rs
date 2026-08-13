mod scope;
pub(crate) mod security;

pub(crate) use scope::PluginScopeResolver;

use crate::AppState;
use crate::error::WebApiError;
use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, Response, StatusCode};
use axum::middleware;
use axum::routing::{delete, get, post};
use bytes::Bytes;
use futures_util::stream;
use ora_contracts::{
    AGENT_INVOCATION_PATH, AGENT_INVOCATIONS_PATH, AgentInvocationRequest,
    AgentInvocationStreamEnvelope, CandidateSelectionView, DataRemovalConfirmationResponse,
    GetPluginLaunchGrantResponse, INVOCATION_ID_HEADER, IdentifyPluginRequest,
    IdentifyPluginResponse, InstallPluginRequest, InstallPluginResponse,
    NativePluginSelectionResponse, PLUGIN_DISABLE_PATH, PLUGIN_ENABLE_PATH, PLUGIN_IDENTIFY_PATH,
    PLUGIN_INSTALL_PATH, PLUGIN_LAUNCH_GRANT_PATH, PLUGIN_PATH, PLUGIN_REMOVE_DATA_PATH,
    PLUGIN_RESET_CRASH_LOOP_PATH, PLUGIN_SCAN_PATH, PLUGIN_START_PATH, PLUGIN_STOP_PATH,
    PLUGINS_PATH, PluginActionResponse, PluginCatalogItem, PluginCatalogResponse,
    PluginDiagnosticView, PluginEnvironmentBinding, PluginLaunchGrantView,
    PluginLaunchValueReference, RemovePluginDataRequest, ScanPluginsRequest, ScanPluginsResponse,
    SetPluginLaunchGrantRequest,
};
use ora_plugin_manager::{
    AgentInvocationCancellation, AgentInvocationHandle, AgentInvocationResult, CandidateHandle,
    DataRemovalScope, DiscoveryRootId, EnvironmentBinding, EnvironmentVariableName,
    LaunchValueReference, ManagementSessionId, PluginError, PluginLaunchGrant, PluginManagement,
    PluginRuntimeInvocation, SelectionHandle, StopReason,
};
use ora_plugin_protocol::{AgentRequest, AgentScope, PluginId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::future::Future;
use std::path::Path as FilePath;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as SyncMutex, PoisonError};
use tokio::sync::{Mutex, mpsc};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Object-safe HTTP adapter boundary; the concrete management/runtime types remain statically dispatched below it.
pub(crate) trait PluginBackend: Send + Sync {
    fn register_native_selection(
        &self,
        path: &FilePath,
    ) -> Result<NativePluginSelectionResponse, PluginError>;
    fn authorize_all_owner_data_removal(
        &self,
        plugin_id: PluginId,
    ) -> Result<DataRemovalConfirmationResponse, PluginError>;
    fn catalog(&self) -> BoxFuture<'_, Result<PluginCatalogResponse, PluginError>>;
    fn scan(
        &self,
        root_ids: Vec<String>,
    ) -> BoxFuture<'_, Result<ScanPluginsResponse, PluginError>>;
    fn identify(
        &self,
        handle: String,
    ) -> BoxFuture<'_, Result<IdentifyPluginResponse, PluginError>>;
    fn install(&self, handle: String) -> BoxFuture<'_, Result<InstallPluginResponse, PluginError>>;
    fn enable(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>>;
    fn disable(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>>;
    fn uninstall(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>>;
    fn set_launch_grant(
        &self,
        plugin_id: String,
        grant: SetPluginLaunchGrantRequest,
    ) -> BoxFuture<'_, Result<(), PluginError>>;
    fn get_launch_grant(
        &self,
        plugin_id: String,
    ) -> BoxFuture<'_, Result<GetPluginLaunchGrantResponse, PluginError>>;
    fn revoke_launch_grant(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>>;
    fn reset_crash_loop(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>>;
    fn remove_data(
        &self,
        plugin_id: String,
        request: RemovePluginDataRequest,
    ) -> BoxFuture<'_, Result<(), PluginError>>;
    fn start(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>>;
    fn stop(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>>;
    fn invoke(
        &self,
        plugin_id: String,
        request: AgentRequest,
    ) -> BoxFuture<'_, Result<AgentInvocationHandle, PluginError>>;
    fn close_admission(&self);
    fn shutdown(&self) -> BoxFuture<'_, Result<(), PluginError>>;
}

/// Concrete application adapter binding one bearer session to management and runtime facades.
pub(crate) struct PluginBackendAdapter<Management, Runtime> {
    management: Arc<Management>,
    runtime: Runtime,
    session: ManagementSessionId,
    closing: AtomicBool,
    destructive_confirmations: DestructiveConfirmationStore,
}

impl<Management, Runtime> PluginBackendAdapter<Management, Runtime> {
    pub(crate) fn new(management: Arc<Management>, runtime: Runtime) -> Result<Self, PluginError> {
        Ok(Self {
            management,
            runtime,
            session: ManagementSessionId::new_random()?,
            closing: AtomicBool::new(false),
            destructive_confirmations: DestructiveConfirmationStore::default(),
        })
    }

    fn ensure_open(&self) -> Result<(), PluginError> {
        if self.closing.load(Ordering::SeqCst) {
            return Err(PluginError::BackendShuttingDown);
        }
        Ok(())
    }
}

impl<Management, Runtime> PluginBackend for PluginBackendAdapter<Management, Runtime>
where
    Management: PluginManagement + Send + Sync + 'static,
    Runtime: PluginRuntimeInvocation + Clone + Send + Sync + 'static,
{
    fn register_native_selection(
        &self,
        path: &FilePath,
    ) -> Result<NativePluginSelectionResponse, PluginError> {
        self.ensure_open()?;
        let selection = self
            .management
            .register_native_selection(self.session.clone(), path)?;
        Ok(NativePluginSelectionResponse {
            selection: Some(candidate_selection_view(selection)),
        })
    }

    fn authorize_all_owner_data_removal(
        &self,
        plugin_id: PluginId,
    ) -> Result<DataRemovalConfirmationResponse, PluginError> {
        self.ensure_open()?;
        let confirmation_handle = random_authority_token()?;
        self.destructive_confirmations
            .insert(confirmation_handle.clone(), plugin_id);
        Ok(DataRemovalConfirmationResponse {
            confirmation_handle,
        })
    }

    fn catalog(&self) -> BoxFuture<'_, Result<PluginCatalogResponse, PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            let catalog = self.management.scan_installed().await?;
            Ok(PluginCatalogResponse {
                revision: catalog.revision,
                plugins: catalog.entries.iter().map(catalog_entry_view).collect(),
            })
        })
    }

    fn scan(
        &self,
        root_ids: Vec<String>,
    ) -> BoxFuture<'_, Result<ScanPluginsResponse, PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            let roots = root_ids
                .into_iter()
                .map(DiscoveryRootId::parse)
                .collect::<Result<Vec<_>, _>>()?;
            let candidates = self
                .management
                .scan_candidates(&self.session, roots)
                .await?;
            Ok(ScanPluginsResponse {
                candidates: candidates
                    .into_iter()
                    .map(candidate_selection_view)
                    .collect(),
            })
        })
    }

    fn identify(
        &self,
        handle: String,
    ) -> BoxFuture<'_, Result<IdentifyPluginResponse, PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            let handle = SelectionHandle::from_opaque(handle)?;
            let identified = self.management.identify(&self.session, handle).await?;
            Ok(IdentifyPluginResponse {
                plugin_id: identified.plugin_id,
                plugin_version: identified.plugin_version,
                content_digest: identified.content_digest,
                candidate_handle: identified.candidate_handle.as_str().to_owned(),
                manifest: identified.package.manifest,
                compatibility: format!("{:?}", identified.package.compatibility),
                support: format!("{:?}", identified.package.support),
                diagnostics: identified
                    .package
                    .diagnostics
                    .iter()
                    .map(diagnostic_view)
                    .collect(),
            })
        })
    }

    fn install(&self, handle: String) -> BoxFuture<'_, Result<InstallPluginResponse, PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            let handle = CandidateHandle::from_opaque(handle)?;
            let installed = self
                .management
                .install_authorized_candidate(&self.session, handle)
                .await?;
            Ok(InstallPluginResponse {
                plugin_id: installed.plugin_id,
                plugin_version: installed.record.plugin_version,
                content_digest: installed.record.content_digest,
                content_owner: installed.record.content_owner,
                enabled: false,
            })
        })
    }

    fn enable(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.management.enable(parse_plugin_id(plugin_id)?).await
        })
    }

    fn disable(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.management.disable(parse_plugin_id(plugin_id)?).await
        })
    }

    fn uninstall(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.management.uninstall(parse_plugin_id(plugin_id)?).await
        })
    }

    fn set_launch_grant(
        &self,
        plugin_id: String,
        grant: SetPluginLaunchGrantRequest,
    ) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            let plugin_id = parse_plugin_id(plugin_id)?;
            let grant = launch_grant_from_request(plugin_id, grant)?;
            self.management.set_launch_grant(grant).await
        })
    }

    fn get_launch_grant(
        &self,
        plugin_id: String,
    ) -> BoxFuture<'_, Result<GetPluginLaunchGrantResponse, PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            let plugin_id = parse_plugin_id(plugin_id)?;
            let grant = self.management.get_launch_grant(&plugin_id).await?;
            Ok(GetPluginLaunchGrantResponse {
                grant: grant.map(launch_grant_view),
            })
        })
    }

    fn revoke_launch_grant(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.management
                .revoke_launch_grant(parse_plugin_id(plugin_id)?)
                .await
        })
    }

    fn reset_crash_loop(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.management
                .reset_crash_loop(parse_plugin_id(plugin_id)?)
                .await
        })
    }

    fn remove_data(
        &self,
        plugin_id: String,
        request: RemovePluginDataRequest,
    ) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            let plugin_id = parse_plugin_id(plugin_id)?;
            let scope = match request {
                RemovePluginDataRequest::CurrentContentOwner {} => {
                    DataRemovalScope::CurrentContentOwner
                }
                RemovePluginDataRequest::AllOwners {
                    confirmation_handle,
                } => {
                    self.destructive_confirmations
                        .consume(&confirmation_handle, &plugin_id)?;
                    DataRemovalScope::AllOwners
                }
            };
            self.management.remove_plugin_data(plugin_id, scope).await
        })
    }

    fn start(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.runtime.start(&parse_plugin_id(plugin_id)?).await
        })
    }

    fn stop(&self, plugin_id: String) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.runtime
                .stop(&parse_plugin_id(plugin_id)?, StopReason::ManualStop)
                .await
        })
    }

    fn invoke(
        &self,
        plugin_id: String,
        request: AgentRequest,
    ) -> BoxFuture<'_, Result<AgentInvocationHandle, PluginError>> {
        Box::pin(async move {
            self.ensure_open()?;
            self.runtime
                .invoke(&parse_plugin_id(plugin_id)?, request)
                .await
        })
    }

    fn close_admission(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.destructive_confirmations.clear();
    }

    fn shutdown(&self) -> BoxFuture<'_, Result<(), PluginError>> {
        Box::pin(async move { self.runtime.shutdown_all().await })
    }
}

#[derive(Default)]
struct DestructiveConfirmationStore {
    entries: SyncMutex<BTreeMap<String, PluginId>>,
}

impl DestructiveConfirmationStore {
    fn insert(&self, handle: String, plugin_id: PluginId) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(handle, plugin_id);
    }

    /// Consumes first so wrong-plugin probes cannot reuse or retarget the bearer capability.
    fn consume(&self, handle: &str, plugin_id: &PluginId) -> Result<(), PluginError> {
        let confirmed_plugin = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(handle)
            .ok_or(PluginError::DestructiveConfirmationInvalid)?;
        if &confirmed_plugin != plugin_id {
            return Err(PluginError::DestructiveConfirmationInvalid);
        }
        Ok(())
    }

    fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

#[derive(Clone, Default)]
pub(crate) struct InvocationRegistry {
    inner: Arc<Mutex<BTreeMap<String, AgentInvocationCancellation>>>,
}

impl InvocationRegistry {
    async fn insert(&self, id: String, cancellation: AgentInvocationCancellation) {
        self.inner.lock().await.insert(id, cancellation);
    }

    async fn get(&self, id: &str) -> Option<AgentInvocationCancellation> {
        self.inner.lock().await.get(id).cloned()
    }

    async fn remove(&self, id: &str) {
        self.inner.lock().await.remove(id);
    }

    /// Cancels every exposed stream so graceful HTTP shutdown can reach its EOF boundary.
    pub(crate) async fn cancel_all(&self) {
        let cancellations = {
            let mut invocations = self.inner.lock().await;
            std::mem::take(&mut *invocations)
                .into_values()
                .collect::<Vec<_>>()
        };
        for cancellation in cancellations {
            let _ = cancellation.cancel().await;
        }
    }
}

/// Builds plugin-only routes and applies the security boundary before every handler.
pub(crate) fn router(app_state: &AppState) -> Option<Router<AppState>> {
    let security = app_state.plugin_security()?.clone();
    let routes = Router::new()
        .route(PLUGINS_PATH, get(catalog).options(preflight_placeholder))
        .route(PLUGIN_SCAN_PATH, post(scan).options(preflight_placeholder))
        .route(
            PLUGIN_IDENTIFY_PATH,
            post(identify).options(preflight_placeholder),
        )
        .route(
            PLUGIN_INSTALL_PATH,
            post(install).options(preflight_placeholder),
        )
        .route(
            PLUGIN_ENABLE_PATH,
            post(enable).options(preflight_placeholder),
        )
        .route(
            PLUGIN_DISABLE_PATH,
            post(disable).options(preflight_placeholder),
        )
        .route(
            PLUGIN_PATH,
            delete(uninstall).options(preflight_placeholder),
        )
        .route(
            PLUGIN_LAUNCH_GRANT_PATH,
            get(get_launch_grant)
                .put(set_launch_grant)
                .delete(revoke_launch_grant)
                .options(preflight_placeholder),
        )
        .route(
            PLUGIN_RESET_CRASH_LOOP_PATH,
            post(reset_crash_loop).options(preflight_placeholder),
        )
        .route(
            PLUGIN_REMOVE_DATA_PATH,
            post(remove_data).options(preflight_placeholder),
        )
        .route(
            PLUGIN_START_PATH,
            post(start).options(preflight_placeholder),
        )
        .route(PLUGIN_STOP_PATH, post(stop).options(preflight_placeholder))
        .route(
            AGENT_INVOCATIONS_PATH,
            post(invoke).options(preflight_placeholder),
        )
        .route(
            AGENT_INVOCATION_PATH,
            delete(cancel_invocation).options(preflight_placeholder),
        )
        .layer(middleware::from_fn_with_state(security, security::enforce));
    Some(routes)
}

async fn preflight_placeholder() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn catalog(
    State(state): State<AppState>,
) -> Result<Json<PluginCatalogResponse>, WebApiError> {
    Ok(Json(backend(&state)?.catalog().await?))
}

async fn scan(
    State(state): State<AppState>,
    Json(request): Json<ScanPluginsRequest>,
) -> Result<Json<ScanPluginsResponse>, WebApiError> {
    Ok(Json(backend(&state)?.scan(request.root_ids).await?))
}

async fn identify(
    State(state): State<AppState>,
    Json(request): Json<IdentifyPluginRequest>,
) -> Result<Json<IdentifyPluginResponse>, WebApiError> {
    Ok(Json(
        backend(&state)?.identify(request.selection_handle).await?,
    ))
}

async fn install(
    State(state): State<AppState>,
    Json(request): Json<InstallPluginRequest>,
) -> Result<Json<InstallPluginResponse>, WebApiError> {
    Ok(Json(
        backend(&state)?.install(request.candidate_handle).await?,
    ))
}

macro_rules! plugin_action {
    ($name:ident, $method:ident) => {
        async fn $name(
            State(state): State<AppState>,
            Path(plugin_id): Path<String>,
        ) -> Result<Json<PluginActionResponse>, WebApiError> {
            backend(&state)?.$method(plugin_id).await?;
            Ok(Json(PluginActionResponse {}))
        }
    };
}

plugin_action!(enable, enable);
plugin_action!(disable, disable);
plugin_action!(uninstall, uninstall);
plugin_action!(revoke_launch_grant, revoke_launch_grant);
plugin_action!(reset_crash_loop, reset_crash_loop);
plugin_action!(start, start);
plugin_action!(stop, stop);

async fn set_launch_grant(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    Json(grant): Json<SetPluginLaunchGrantRequest>,
) -> Result<Json<PluginActionResponse>, WebApiError> {
    backend(&state)?.set_launch_grant(plugin_id, grant).await?;
    Ok(Json(PluginActionResponse {}))
}

async fn get_launch_grant(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
) -> Result<Json<GetPluginLaunchGrantResponse>, WebApiError> {
    Ok(Json(backend(&state)?.get_launch_grant(plugin_id).await?))
}

async fn remove_data(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    Json(request): Json<RemovePluginDataRequest>,
) -> Result<Json<PluginActionResponse>, WebApiError> {
    backend(&state)?.remove_data(plugin_id, request).await?;
    Ok(Json(PluginActionResponse {}))
}

async fn invoke(
    State(state): State<AppState>,
    Json(request): Json<AgentInvocationRequest>,
) -> Result<Response<Body>, WebApiError> {
    let scope = state.plugin_scope_resolver().resolve(request.scope).await?;
    let params = inject_resolved_scope(request.params, scope)?;
    let agent_request = AgentRequest::from_method_params(request.method, params)
        .map_err(|_| WebApiError::bad_request("Agent request params are invalid"))?;
    let mut handle = backend(&state)?
        .invoke(request.plugin_id.to_string(), agent_request)
        .await?;
    let invocation_id = random_invocation_id()?;
    let cancellation = handle.cancellation();
    let registry = state.plugin_invocations().clone();
    registry
        .insert(invocation_id.clone(), cancellation.clone())
        .await;
    let (sender, receiver) = mpsc::channel::<Result<Bytes, Infallible>>(16);
    let task_invocation_id = invocation_id.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = sender.closed() => {
                    let _ = cancellation.cancel().await;
                    registry.remove(&task_invocation_id).await;
                    return;
                }
                event = handle.next_event() => {
                    let Some(event) = event else { break; };
                    if send_line(&sender, &AgentInvocationStreamEnvelope::Event { event }).await.is_err() {
                        let _ = cancellation.cancel().await;
                        registry.remove(&task_invocation_id).await;
                        return;
                    }
                }
            }
        }
        let terminal = tokio::select! {
            _ = sender.closed() => {
                let _ = cancellation.cancel().await;
                None
            }
            result = handle.finish() => Some(match result {
                Ok(AgentInvocationResult::Response(response)) => match response.to_result_value() {
                    Ok(result) => AgentInvocationStreamEnvelope::Completed { result },
                    Err(_) => AgentInvocationStreamEnvelope::Failed { error: "serialization_failure".to_owned() },
                },
                Ok(AgentInvocationResult::Turn(result)) => match serde_json::to_value(result) {
                    Ok(result) => AgentInvocationStreamEnvelope::Completed { result },
                    Err(_) => AgentInvocationStreamEnvelope::Failed { error: "serialization_failure".to_owned() },
                },
                Err(error) => AgentInvocationStreamEnvelope::Failed { error: plugin_error_code(&error).to_owned() },
            }),
        };
        if let Some(terminal) = terminal {
            let _ = send_line(&sender, &terminal).await;
        }
        registry.remove(&task_invocation_id).await;
    });
    let stream = stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    });
    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson"),
    );
    response.headers_mut().insert(
        INVOCATION_ID_HEADER,
        HeaderValue::from_str(&invocation_id)
            .map_err(|_| WebApiError::bad_request("invalid invocation id"))?,
    );
    Ok(response)
}

async fn cancel_invocation(
    State(state): State<AppState>,
    Path(invocation_id): Path<String>,
) -> Result<Json<PluginActionResponse>, WebApiError> {
    let cancellation = state
        .plugin_invocations()
        .get(&invocation_id)
        .await
        .ok_or_else(|| WebApiError::not_found("plugin invocation was not found"))?;
    cancellation.cancel().await?;
    Ok(Json(PluginActionResponse {}))
}

async fn send_line(
    sender: &mpsc::Sender<Result<Bytes, Infallible>>,
    value: &AgentInvocationStreamEnvelope,
) -> Result<(), ()> {
    let mut bytes = serde_json::to_vec(&value).map_err(|_| ())?;
    bytes.push(b'\n');
    sender.send(Ok(Bytes::from(bytes))).await.map_err(|_| ())
}

fn backend(state: &AppState) -> Result<&Arc<dyn PluginBackend>, WebApiError> {
    state
        .plugin_backend()
        .ok_or_else(|| WebApiError::unavailable("plugin backend is unavailable"))
}

fn parse_plugin_id(value: String) -> Result<PluginId, PluginError> {
    PluginId::parse(value).map_err(|error| PluginError::Internal {
        message: error.to_string(),
    })
}

fn candidate_selection_view(
    selection: ora_plugin_manager::CandidateSelection,
) -> CandidateSelectionView {
    CandidateSelectionView {
        selection_handle: selection.selection_handle.as_str().to_owned(),
        display_name: selection.display_name,
    }
}

fn catalog_entry_view(entry: &ora_plugin_manager::CatalogEntry) -> PluginCatalogItem {
    PluginCatalogItem {
        plugin_id: entry.plugin_id.clone(),
        manifest: entry.manifest.clone(),
        validity: format!("{:?}", entry.validity),
        compatibility: format!("{:?}", entry.compatibility),
        support: format!("{:?}", entry.support),
        integrity: format!("{:?}", entry.integrity),
        diagnostics: entry.diagnostics.iter().map(diagnostic_view).collect(),
    }
}

fn diagnostic_view(diagnostic: &ora_plugin_manager::PluginDiagnostic) -> PluginDiagnosticView {
    PluginDiagnosticView {
        code: format!("{:?}", diagnostic.code),
        message: diagnostic.message.clone(),
    }
}

/// Reconstructs durable management grant types without accepting a client-authored route id.
fn launch_grant_from_request(
    plugin_id: PluginId,
    request: SetPluginLaunchGrantRequest,
) -> Result<PluginLaunchGrant, PluginError> {
    let environment = request
        .environment
        .into_iter()
        .map(|binding| {
            Ok(EnvironmentBinding {
                target: EnvironmentVariableName::parse(binding.target)
                    .map_err(|_| PluginError::InvalidLaunchGrant)?,
                value: launch_value_reference_from_contract(binding.value),
            })
        })
        .collect::<Result<Vec<_>, PluginError>>()?;
    Ok(PluginLaunchGrant {
        plugin_id,
        content_owner: request.content_owner,
        schema_version: request.schema_version,
        revision: request.revision,
        environment,
    })
}

fn launch_value_reference_from_contract(
    reference: PluginLaunchValueReference,
) -> LaunchValueReference {
    match reference {
        PluginLaunchValueReference::HostConfiguration { key } => {
            LaunchValueReference::HostConfiguration { key }
        }
        PluginLaunchValueReference::Credential { key } => LaunchValueReference::Credential { key },
        PluginLaunchValueReference::DiscoveredExecutable { provider } => {
            LaunchValueReference::DiscoveredExecutable { provider }
        }
        PluginLaunchValueReference::AuthorizedPath { path_id } => {
            LaunchValueReference::AuthorizedPath { path_id }
        }
    }
}

fn launch_grant_view(grant: PluginLaunchGrant) -> PluginLaunchGrantView {
    PluginLaunchGrantView {
        plugin_id: grant.plugin_id,
        content_owner: grant.content_owner,
        schema_version: grant.schema_version,
        revision: grant.revision,
        environment: grant
            .environment
            .into_iter()
            .map(|binding| PluginEnvironmentBinding {
                target: binding.target.as_str().to_owned(),
                value: launch_value_reference_view(binding.value),
            })
            .collect(),
    }
}

fn launch_value_reference_view(reference: LaunchValueReference) -> PluginLaunchValueReference {
    match reference {
        LaunchValueReference::HostConfiguration { key } => {
            PluginLaunchValueReference::HostConfiguration { key }
        }
        LaunchValueReference::Credential { key } => PluginLaunchValueReference::Credential { key },
        LaunchValueReference::DiscoveredExecutable { provider } => {
            PluginLaunchValueReference::DiscoveredExecutable { provider }
        }
        LaunchValueReference::AuthorizedPath { path_id } => {
            PluginLaunchValueReference::AuthorizedPath { path_id }
        }
    }
}

/// Injects the Host-resolved scope and rejects attempts to smuggle a second scope in params.
fn inject_resolved_scope(mut params: Value, scope: AgentScope) -> Result<Value, WebApiError> {
    let object = params
        .as_object_mut()
        .ok_or_else(|| WebApiError::bad_request("Agent request params must be an object"))?;
    if object.contains_key("scope") {
        return Err(WebApiError::bad_request(
            "Agent request scope must use the application scope field",
        ));
    }
    object.insert(
        "scope".to_owned(),
        serde_json::to_value(scope).map_err(|error| {
            WebApiError::internal("resolved Agent scope serialization failed", error)
        })?,
    );
    Ok(params)
}

fn random_invocation_id() -> Result<String, WebApiError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| WebApiError::unavailable("invocation identity is unavailable"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn random_authority_token() -> Result<String, PluginError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| PluginError::Internal {
        message: "authorization identity is unavailable".to_owned(),
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn plugin_error_code(error: &PluginError) -> &'static str {
    match error {
        PluginError::Cancelled { .. } => "cancelled",
        PluginError::RequestTimedOut { .. } => "request_timed_out",
        PluginError::UnknownOutcome { .. } => "unknown_outcome",
        PluginError::AgentBusinessFailure { .. } => "agent_business_failure",
        PluginError::PluginBusy { .. } => "plugin_busy",
        PluginError::Disabled { .. } => "plugin_disabled",
        _ => "plugin_failure",
    }
}

#[cfg(test)]
mod tests {
    use super::{DestructiveConfirmationStore, inject_resolved_scope};
    use ora_plugin_manager::PluginError;
    use ora_plugin_protocol::{AgentScope, PluginId};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    /// Application params cannot override the Host-resolved Agent scope.
    #[test]
    fn rejects_scope_smuggling_inside_agent_params() {
        let result = inject_resolved_scope(
            json!({ "providerId": "example", "scope": { "type": "global" } }),
            AgentScope::Global {},
        );

        assert!(result.is_err());
    }

    /// A destructive confirmation is bound to one plugin and consumed on every attempt.
    #[test]
    fn destructive_confirmation_is_plugin_bound_and_single_use() {
        let store = DestructiveConfirmationStore::default();
        let first = PluginId::parse("ora.first")
            .unwrap_or_else(|error| panic!("expected first plugin id: {error}"));
        let second = PluginId::parse("ora.second")
            .unwrap_or_else(|error| panic!("expected second plugin id: {error}"));
        store.insert("confirmation".to_owned(), first.clone());

        assert_eq!(
            (
                store.consume("confirmation", &second),
                store.consume("confirmation", &first),
            ),
            (
                Err(PluginError::DestructiveConfirmationInvalid),
                Err(PluginError::DestructiveConfirmationInvalid),
            )
        );
    }
}
