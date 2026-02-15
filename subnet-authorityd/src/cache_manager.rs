use std::thread;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use anyhow::Result;
use shared::types::ServiceEntry;
use crate::cache::{db::CacheDb, hash};
use crate::config::CacheConfig;
// Fix #5: import BrowserEvent from its owning module
pub use crate::mdns::browser::BrowserEvent;

/// Commands sent to the cache thread
pub enum CacheCommand {
    Upsert(ServiceEntry, oneshot::Sender<Result<bool>>),
    MarkDead(String, oneshot::Sender<Result<()>>),
    GetAll(oneshot::Sender<Result<Vec<ServiceEntry>>>),
    GetByType(String, oneshot::Sender<Result<Vec<ServiceEntry>>>),
    GetOne(String, oneshot::Sender<Result<Option<ServiceEntry>>>),
    Maintenance {
        stale_after_secs: u64,
        prune_after_secs: u64,
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown,
}

/// Handle to interact with the cache database
#[derive(Clone)]
pub struct CacheHandle {
    tx: mpsc::Sender<CacheCommand>,
}

impl CacheHandle {
    /// Spawn a new cache thread with the given database
    pub fn spawn(db: CacheDb, hash_tx: watch::Sender<String>) -> Self {
        let (tx, mut rx) = mpsc::channel::<CacheCommand>(256);

        // Fix #2: helper to recompute hash only after mutations
        let recompute_hash = |db: &CacheDb, hash_tx: &watch::Sender<String>| {
            if let Ok(services) = db.get_all_services() {
                let new_hash = hash::compute_hash(&services);
                let _ = hash_tx.send(new_hash);
            }
        };

        thread::spawn(move || {
            while let Some(cmd) = rx.blocking_recv() {
                match cmd {
                    CacheCommand::Upsert(entry, reply) => {
                        let result = db.upsert_service(&entry);
                        // Fix #2: only recompute hash when data actually changed
                        if matches!(&result, Ok(true)) {
                            recompute_hash(&db, &hash_tx);
                        }
                        let _ = reply.send(result);
                    }
                    CacheCommand::MarkDead(instance_name, reply) => {
                        let result = db.mark_dead(&instance_name);
                        if result.is_ok() {
                            recompute_hash(&db, &hash_tx);
                        }
                        let _ = reply.send(result);
                    }
                    CacheCommand::GetAll(reply) => {
                        let result = db.get_all_services();
                        let _ = reply.send(result);
                    }
                    CacheCommand::GetByType(service_type, reply) => {
                        let result = db.get_services_by_type(&service_type);
                        let _ = reply.send(result);
                    }
                    CacheCommand::GetOne(instance_name, reply) => {
                        let result = db.get_service(&instance_name);
                        let _ = reply.send(result);
                    }
                    CacheCommand::Maintenance { stale_after_secs, prune_after_secs, reply } => {
                        let result = (|| {
                            db.mark_stale(stale_after_secs)?;
                            db.prune_stale(prune_after_secs)?;
                            Ok(())
                        })();
                        if result.is_ok() {
                            recompute_hash(&db, &hash_tx);
                        }
                        let _ = reply.send(result);
                    }
                    CacheCommand::Shutdown => {
                        tracing::info!("Cache thread shutting down");
                        break;
                    }
                }
            }
        });

        Self { tx }
    }

    /// Insert or update a service. Returns true if data changed.
    pub async fn upsert(&self, entry: ServiceEntry) -> Result<bool> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CacheCommand::Upsert(entry, reply)).await?;
        rx.await?
    }

    /// Mark a service as dead
    pub async fn mark_dead(&self, instance_name: String) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CacheCommand::MarkDead(instance_name, reply)).await?;
        rx.await?
    }

    /// Get all services
    pub async fn get_all(&self) -> Result<Vec<ServiceEntry>> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CacheCommand::GetAll(reply)).await?;
        rx.await?
    }

    /// Get services by type
    pub async fn get_by_type(&self, service_type: String) -> Result<Vec<ServiceEntry>> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CacheCommand::GetByType(service_type, reply)).await?;
        rx.await?
    }

    /// Get a single service by instance name
    pub async fn get_one(&self, instance_name: String) -> Result<Option<ServiceEntry>> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CacheCommand::GetOne(instance_name, reply)).await?;
        rx.await?
    }

    /// Run maintenance (mark stale, prune old)
    pub async fn maintenance(&self, stale_after_secs: u64, prune_after_secs: u64) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CacheCommand::Maintenance {
            stale_after_secs,
            prune_after_secs,
            reply,
        }).await?;
        rx.await?
    }

    /// Shutdown the cache thread
    pub async fn shutdown(&self) -> Result<()> {
        self.tx.send(CacheCommand::Shutdown).await?;
        Ok(())
    }
}

/// Cache manager event loop - bridges browser events to cache
pub async fn run(
    cache: CacheHandle,
    mut rx: mpsc::Receiver<BrowserEvent>,
    config: CacheConfig,
    cancel: CancellationToken,
) -> Result<()> {
    // Fix #6: use dedicated maintenance interval instead of browse_interval_secs
    let mut maintenance_interval = tokio::time::interval(
        std::time::Duration::from_secs(config.maintenance_interval_secs)
    );

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    BrowserEvent::Resolved(entry) => {
                        if let Err(e) = cache.upsert(entry).await {
                            tracing::error!("Failed to upsert service: {}", e);
                        }
                    }
                    BrowserEvent::Removed(instance_name) => {
                        if let Err(e) = cache.mark_dead(instance_name).await {
                            tracing::error!("Failed to mark service as dead: {}", e);
                        }
                    }
                }
            }
            _ = maintenance_interval.tick() => {
                if let Err(e) = cache.maintenance(
                    config.stale_after_secs,
                    config.prune_after_secs
                ).await {
                    tracing::error!("Failed to run maintenance: {}", e);
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("Cache manager shutting down");
                break;
            }
        }
    }

    Ok(())
}
