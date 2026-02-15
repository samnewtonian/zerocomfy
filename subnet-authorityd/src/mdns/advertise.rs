use std::collections::HashMap;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use anyhow::{Context, Result};
use shared::protocol::{AUTHORITY_SERVICE_TYPE, TXT_ZONE, TXT_PREFIX};
use crate::config::AuthorityConfig;

pub fn register_authority(
    daemon: &ServiceDaemon,
    config: &AuthorityConfig,
    api_port: u16,
) -> Result<ServiceInfo> {
    let hostname = hostname::get()
        .context("Failed to get system hostname")?
        .to_string_lossy()
        .to_string();

    let instance_name = format!("subnet-authority-{}", hostname);

    // Create TXT records with zone and prefix info
    let txt_records = HashMap::from([
        (TXT_ZONE.to_string(), config.zone.clone()),
        (TXT_PREFIX.to_string(), config.prefix.clone()),
    ]);

    let service_info = ServiceInfo::new(
        AUTHORITY_SERVICE_TYPE,
        &instance_name,
        &hostname,
        &config.address,
        api_port,
        txt_records,
    )
    .context("Failed to create ServiceInfo")?;

    daemon
        .register(service_info.clone())
        .context("Failed to register mDNS service")?;

    tracing::info!(
        "Registered {} as {} on port {}",
        AUTHORITY_SERVICE_TYPE,
        instance_name,
        api_port
    );

    Ok(service_info)
}

pub fn unregister_authority(daemon: &ServiceDaemon, service_info: &ServiceInfo) -> Result<()> {
    daemon
        .unregister(service_info.get_fullname())
        .context("Failed to unregister mDNS service")?;

    tracing::info!("Unregistered {}", service_info.get_fullname());
    Ok(())
}
