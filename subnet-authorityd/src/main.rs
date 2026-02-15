mod config;
mod cache;
mod cache_manager;
mod mdns;
mod api;

use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use mdns_sd::ServiceDaemon;
use anyhow::{Context, Result};
use crate::cache::db::CacheDb;
use crate::cache_manager::CacheHandle;
use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("subnet_authorityd=info"))
        )
        .init();

    tracing::info!("Starting subnet-authorityd");

    // Load config
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/subnet-authority/authorityd.toml".to_string());

    let config = Config::load(&config_path)
        .with_context(|| format!("Failed to load config from {}", config_path))?;

    tracing::info!("Loaded config from {}", config_path);

    // Open SQLite database
    let db = CacheDb::open(&config.cache.db_path)?;
    tracing::info!("Opened database at {:?}", config.cache.db_path);

    // Compute initial hash
    let initial_services = db.get_all_services()?;
    let initial_hash = cache::hash::compute_hash(&initial_services);
    tracing::info!("Initial cache hash: {}", initial_hash);

    // Create hash watch channel
    let (hash_tx, hash_rx) = watch::channel(initial_hash);

    // Start cache manager thread
    let cache_handle = CacheHandle::spawn(db, hash_tx);

    // Create mDNS daemon bound to configured interface
    let mdns_daemon = ServiceDaemon::new()
        .context("Failed to create mDNS daemon")?;
    mdns_daemon
        .disable_interface(mdns_sd::IfKind::All)
        .context("Failed to disable default interfaces")?;
    mdns_daemon
        .enable_interface(config.authority.interface.as_str())
        .with_context(|| format!("Failed to enable interface {}", config.authority.interface))?;

    // Extract port from listen address
    let api_port = config.api.listen
        .split(':')
        .last()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8053);

    // Register self-advertisement
    let service_info = mdns::advertise::register_authority(
        &mdns_daemon,
        &config.authority,
        api_port,
    )?;

    // Create cancellation token for graceful shutdown
    let cancel = CancellationToken::new();

    // Spawn mDNS browser task
    let (browser_tx, browser_rx) = mpsc::channel(256);
    let browser_cancel = cancel.clone();
    let browser_daemon = mdns_daemon.clone();
    let browser_handle = tokio::spawn(async move {
        if let Err(e) = mdns::browser::run_browser(browser_daemon, browser_tx, browser_cancel).await {
            tracing::error!("mDNS browser error: {}", e);
        }
    });

    // Spawn cache manager task
    let mgr_cancel = cancel.clone();
    let mgr_config = config.cache.clone();
    let mgr_cache = cache_handle.clone();
    let mgr_handle = tokio::spawn(async move {
        if let Err(e) = cache_manager::run(mgr_cache, browser_rx, mgr_config, mgr_cancel).await {
            tracing::error!("Cache manager error: {}", e);
        }
    });

    // Build API router
    let app_state = api::routes::AppState {
        cache: cache_handle.clone(),
        hash_rx,
        config: Arc::new(config.authority.clone()),
        api_port, // Fix #1: pass pre-computed port to AppState
    };
    let app = api::routes::router(app_state);

    // Bind HTTP server
    let listener = tokio::net::TcpListener::bind(&config.api.listen)
        .await
        .with_context(|| format!("Failed to bind to {}", config.api.listen))?;

    tracing::info!("API listening on {}", config.api.listen);

    // Run server with graceful shutdown
    let server_cancel = cancel.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(async move { server_cancel.cancelled().await })
            .await
        {
            tracing::error!("Server error: {}", e);
        }
    });

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .context("Failed to listen for ctrl-c")?;

    tracing::info!("Shutdown signal received");

    // Trigger cancellation
    cancel.cancel();

    // Wait for all tasks to complete
    let _ = tokio::join!(browser_handle, mgr_handle, server_handle);

    // Unregister mDNS service
    if let Err(e) = mdns::advertise::unregister_authority(&mdns_daemon, &service_info) {
        tracing::error!("Failed to unregister mDNS service: {}", e);
    }

    // Shutdown cache thread
    if let Err(e) = cache_handle.shutdown().await {
        tracing::error!("Failed to shutdown cache: {}", e);
    }

    // Shutdown mDNS daemon
    if let Err(e) = mdns_daemon.shutdown() {
        tracing::error!("Failed to shutdown mDNS daemon: {}", e);
    }

    tracing::info!("Shutdown complete");
    Ok(())
}
