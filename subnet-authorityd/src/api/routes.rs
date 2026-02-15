use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use crate::cache_manager::CacheHandle;
use crate::config::AuthorityConfig;
use shared::types::ServiceEntry;

#[derive(Clone)]
pub struct AppState {
    pub cache: CacheHandle,
    pub hash_rx: watch::Receiver<String>,
    pub config: Arc<AuthorityConfig>,
    /// Fix #1: store api_port directly instead of parsing it from config.zone
    pub api_port: u16,
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub zone: String,
    pub prefix: String,
    pub api_port: u16,
}

#[derive(Deserialize)]
pub struct ServiceQuery {
    #[serde(rename = "type")]
    pub service_type: Option<String>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/config", get(get_config))
        .route("/v1/services", get(get_services))
        .route("/v1/services/hash", get(get_hash))
        .route("/v1/services/:instance", get(get_service))
        .with_state(state)
}

async fn get_config(State(state): State<AppState>) -> Json<ConfigResponse> {
    // Fix #1: use pre-computed api_port from AppState
    Json(ConfigResponse {
        zone: state.config.zone.clone(),
        prefix: state.config.prefix.clone(),
        api_port: state.api_port,
    })
}

async fn get_services(
    State(state): State<AppState>,
    Query(params): Query<ServiceQuery>,
) -> Result<Json<Vec<ServiceEntry>>, StatusCode> {
    let services = if let Some(service_type) = params.service_type {
        state.cache.get_by_type(service_type).await
    } else {
        state.cache.get_all().await
    };

    services
        .map(Json)
        .map_err(|e| {
            tracing::error!("Failed to query services: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_hash(State(state): State<AppState>) -> String {
    state.hash_rx.borrow().clone()
}

async fn get_service(
    State(state): State<AppState>,
    Path(instance): Path<String>,
) -> Result<Json<ServiceEntry>, StatusCode> {
    state
        .cache
        .get_one(instance)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query service: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
