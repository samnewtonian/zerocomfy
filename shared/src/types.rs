use std::collections::HashMap;
use std::net::Ipv6Addr;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// A discovered service on the network.
/// This is the canonical data model used by the authority daemon, API, and client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// Service type, e.g. "_http._tcp"
    pub service_type: String,

    /// Full DNS-SD instance name, e.g. "fileserver._http._tcp.local."
    pub instance_name: String,

    /// Hostname, e.g. "nas.local."
    pub hostname: String,

    /// IPv6 addresses (ULA subnet only)
    pub addresses: Vec<Ipv6Addr>,

    /// Service port
    pub port: u16,

    /// TXT record key-value pairs
    pub txt: HashMap<String, String>,

    /// First time this service was seen
    pub first_seen: DateTime<Utc>,

    /// Last time this service was seen
    pub last_seen: DateTime<Utc>,

    /// TTL in seconds
    pub ttl: u32,

    /// Whether the service is currently alive
    pub alive: bool,
}
