use crate::app_state::AppState;
use crate::error::WebApiError;
use axum::Json;
use axum::extract::{Query, State};
use ora_contracts::{ListSpecsRequest, ListSpecsResponse, ReadSpecRequest, ReadSpecResponse};

/// Lists the specs discoverable in the workspace the query scopes itself to.
pub async fn list_specs(
    State(app_state): State<AppState>,
    Query(request): Query<ListSpecsRequest>,
) -> Result<Json<ListSpecsResponse>, WebApiError> {
    let backend = app_state.backend().clone();

    // A cold catalog walks the workspace and hashes every candidate, which blocks long
    // enough on large repositories to keep off Tokio's async worker threads.
    tokio::task::spawn_blocking(move || backend.list_specs(request))
        .await
        .map_err(|source| WebApiError::internal("spec catalog worker failed", source))?
        .map(Json::from)
        .map_err(WebApiError::from)
}

/// Reads one discovered spec together with its markdown body.
pub async fn read_spec(
    State(app_state): State<AppState>,
    Query(request): Query<ReadSpecRequest>,
) -> Result<Json<ReadSpecResponse>, WebApiError> {
    let backend = app_state.backend().clone();

    tokio::task::spawn_blocking(move || backend.read_spec(request))
        .await
        .map_err(|source| WebApiError::internal("spec catalog worker failed", source))?
        .map(Json::from)
        .map_err(WebApiError::from)
}
