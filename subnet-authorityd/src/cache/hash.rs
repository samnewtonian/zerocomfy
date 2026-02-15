use std::collections::HashMap;
use std::net::Ipv6Addr;
use serde::Serialize;
use sha2::{Sha256, Digest};
use shared::types::ServiceEntry;

/// Fix #3: hash only stable fields — last_seen/first_seen/ttl change on every
/// re-resolve but don't represent meaningful service data changes.
#[derive(Serialize)]
struct HashView<'a> {
    service_type: &'a str,
    instance_name: &'a str,
    hostname: &'a str,
    addresses: &'a [Ipv6Addr],
    port: u16,
    txt: &'a HashMap<String, String>,
    alive: bool,
}

/// Computes a SHA-256 hash of the service list.
/// Services are sorted by instance_name for deterministic output.
/// Fix #8: sort indices instead of cloning the entire service list.
pub fn compute_hash(services: &[ServiceEntry]) -> String {
    let mut indices: Vec<usize> = (0..services.len()).collect();
    indices.sort_by(|&a, &b| services[a].instance_name.cmp(&services[b].instance_name));

    let views: Vec<HashView<'_>> = indices
        .iter()
        .map(|&i| {
            let s = &services[i];
            HashView {
                service_type: &s.service_type,
                instance_name: &s.instance_name,
                hostname: &s.hostname,
                addresses: &s.addresses,
                port: s.port,
                txt: &s.txt,
                alive: s.alive,
            }
        })
        .collect();

    let json = serde_json::to_string(&views)
        .expect("Failed to serialize services for hashing");

    let hash = Sha256::digest(json.as_bytes());
    hex::encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::net::Ipv6Addr;
    use chrono::Utc;

    fn test_entry(instance_name: &str) -> ServiceEntry {
        ServiceEntry {
            service_type: "_http._tcp".to_string(),
            instance_name: instance_name.to_string(),
            hostname: "test.local.".to_string(),
            addresses: vec![Ipv6Addr::new(0xfd00, 0, 0, 1, 0, 0, 0, 1)],
            port: 8080,
            txt: HashMap::new(),
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            ttl: 4500,
            alive: true,
        }
    }

    #[test]
    fn test_hash_deterministic() {
        let entry1 = test_entry("a._http._tcp.local.");
        let entry2 = test_entry("b._http._tcp.local.");

        let hash1 = compute_hash(&[entry1.clone(), entry2.clone()]);
        let hash2 = compute_hash(&[entry2.clone(), entry1.clone()]);

        assert_eq!(hash1, hash2, "Hash should be same regardless of input order");
    }

    #[test]
    fn test_hash_changes_on_modification() {
        let entry1 = test_entry("a._http._tcp.local.");
        let mut entry2 = test_entry("a._http._tcp.local.");

        let hash1 = compute_hash(&[entry1.clone()]);

        entry2.port = 9090;
        let hash2 = compute_hash(&[entry2]);

        assert_ne!(hash1, hash2, "Hash should change when entry changes");
    }

    #[test]
    fn test_hash_stable_across_timestamp_changes() {
        // Fix #3: verify that last_seen/first_seen/ttl don't affect the hash
        let entry1 = test_entry("a._http._tcp.local.");
        let mut entry2 = test_entry("a._http._tcp.local.");
        entry2.last_seen = Utc::now() + chrono::Duration::seconds(60);
        entry2.first_seen = Utc::now() - chrono::Duration::seconds(60);
        entry2.ttl = 9999;

        let hash1 = compute_hash(&[entry1]);
        let hash2 = compute_hash(&[entry2]);

        assert_eq!(hash1, hash2, "Hash should not change when only timestamps/ttl change");
    }
}
