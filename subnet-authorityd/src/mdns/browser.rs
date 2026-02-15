use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use futures::stream::{FuturesUnordered, StreamExt};
use futures::Future;
use anyhow::{Context, Result};
use chrono::Utc;
use shared::types::ServiceEntry;
use std::collections::HashMap;

const META_QUERY_TYPE: &str = "_services._dns-sd._udp.local.";

/// Fix #5: BrowserEvent belongs in the browser module, not cache_manager
pub enum BrowserEvent {
    Resolved(ServiceEntry),
    Removed(String),
}

type RecvResult = (usize, flume::Receiver<ServiceEvent>, std::result::Result<ServiceEvent, flume::RecvError>);
type RecvFuture = Pin<Box<dyn Future<Output = RecvResult> + Send>>;

/// Each future owns a clone of the receiver, avoiding borrow issues with the
/// receivers vec. flume::Receiver is Clone (multi-consumer).
fn make_recv_future(idx: usize, rx: flume::Receiver<ServiceEvent>) -> RecvFuture {
    Box::pin(async move {
        let result = rx.recv_async().await;
        (idx, rx, result)
    })
}

pub async fn run_browser(
    daemon: ServiceDaemon,
    tx: mpsc::Sender<BrowserEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    tracing::info!("Starting mDNS browser");

    // Start browsing the meta-query to discover all service types
    let meta_receiver = daemon
        .browse(META_QUERY_TYPE)
        .context("Failed to start meta-query browse")?;

    let mut browsed_types = HashSet::new();
    let mut next_idx = 0usize;
    // Fix #4: use FuturesUnordered for async event-driven reception instead of
    // try_recv + sleep polling. Each future yields (receiver_index, receiver, result).
    let mut type_futures: FuturesUnordered<RecvFuture> = FuturesUnordered::new();

    loop {
        tokio::select! {
            // Check for new service types from meta-query
            event = meta_receiver.recv_async() => {
                match event {
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        let service_type = info.get_type();

                        if !browsed_types.contains(service_type) {
                            tracing::info!("Discovered new service type: {}", service_type);
                            browsed_types.insert(service_type.to_string());

                            match daemon.browse(service_type) {
                                Ok(receiver) => {
                                    let idx = next_idx;
                                    next_idx += 1;
                                    type_futures.push(make_recv_future(idx, receiver));
                                }
                                Err(e) => {
                                    tracing::error!("Failed to browse {}: {}", service_type, e);
                                }
                            }
                        }
                    }
                    Ok(ServiceEvent::ServiceRemoved(_, _)) => {}
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Error receiving meta-query event: {}", e);
                    }
                }
            }

            // Fix #4: async event-driven instead of polling
            Some((idx, rx, result)) = type_futures.next() => {
                match result {
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        if let Some(entry) = convert_service_info(&info) {
                            tracing::debug!("Resolved service: {}", entry.instance_name);
                            if let Err(e) = tx.send(BrowserEvent::Resolved(entry)).await {
                                tracing::error!("Failed to send resolved event: {}", e);
                            }
                        }
                        type_futures.push(make_recv_future(idx, rx));
                    }
                    Ok(ServiceEvent::ServiceRemoved(_typ, fullname)) => {
                        tracing::debug!("Service removed: {}", fullname);
                        if let Err(e) = tx.send(BrowserEvent::Removed(fullname)).await {
                            tracing::error!("Failed to send removed event: {}", e);
                        }
                        type_futures.push(make_recv_future(idx, rx));
                    }
                    Ok(_) => {
                        type_futures.push(make_recv_future(idx, rx));
                    }
                    Err(e) => {
                        tracing::warn!("Receiver {} disconnected: {}", idx, e);
                    }
                }
            }

            _ = cancel.cancelled() => {
                tracing::info!("mDNS browser shutting down");
                break;
            }
        }
    }

    Ok(())
}

/// Convert an mdns-sd ServiceInfo to our ServiceEntry
fn convert_service_info(info: &mdns_sd::ServiceInfo) -> Option<ServiceEntry> {
    let now = Utc::now();

    // Extract IPv6 addresses only
    let addresses: Vec<Ipv6Addr> = info
        .get_addresses()
        .iter()
        .filter_map(|addr| match addr {
            std::net::IpAddr::V6(ipv6) => Some(*ipv6),
            _ => None,
        })
        .collect();

    if addresses.is_empty() {
        tracing::debug!("Skipping service {} - no IPv6 addresses", info.get_fullname());
        return None;
    }

    // Extract TXT records
    let txt: HashMap<String, String> = info
        .get_properties()
        .iter()
        .map(|prop| {
            let key = prop.key().to_string();
            let value = prop.val_str().to_string();
            (key, value)
        })
        .collect();

    Some(ServiceEntry {
        service_type: info.get_type().to_string(),
        instance_name: info.get_fullname().to_string(),
        hostname: info.get_hostname().to_string(),
        addresses,
        port: info.get_port(),
        txt,
        first_seen: now,
        last_seen: now,
        ttl: 4500, // Default TTL - mdns-sd doesn't expose this
        alive: true,
    })
}
