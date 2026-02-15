# subnet-authority

A lightweight service discovery system for IPv6 ULA subnets. Bridges mDNS/DNS-SD to devices that can't participate in it (embedded systems, tunneled remote clients).

## Status

**In development** — MVP implementation complete:

- ✅ Cargo workspace structure
- ✅ `subnet-config` shell script (system daemon config generator)
- ✅ `subnet-authorityd` core daemon (mDNS browsing, SQLite cache, REST API, self-advertisement)
- 🚧 DNS serving (not yet implemented)
- 🚧 CoAP interface (not yet implemented)
- 🚧 `subnet-client` agent (not yet implemented)
- 🚧 Failover (not yet implemented)

## Quick Start

### Build

```bash
cargo build --release
```

### Configure

Create a config file (see `examples/authorityd.toml`):

```toml
[authority]
interface = "eth0"
prefix = "fd00:1234:5678:1::/64"
address = "fd00:1234:5678:1::1/64"
zone = "subnet.example"

[cache]
db_path = "/var/lib/subnet-authority/services.db"

[api]
listen = "[::]:8053"
```

### Run

```bash
./target/release/subnet-authorityd /path/to/authorityd.toml
```

### Test the API

```bash
# Get authority config
curl http://localhost:8053/v1/config

# List all services
curl http://localhost:8053/v1/services

# Get cache hash (for change detection)
curl http://localhost:8053/v1/services/hash

# Filter by service type
curl 'http://localhost:8053/v1/services?type=_http._tcp'
```

## Architecture

The project consists of three segments:

### 1. subnet-config (Setup Utility)

One-shot CLI tool (implemented as a shell script). Reads a declarative config file and generates native config files for system daemons:

- `radvd.conf` — IPv6 Router Advertisement daemon
- `systemd-networkd` units — Static address configuration
- `dnsmasq.conf` — DNS server configuration

**Usage:**

```bash
./subnet-config generate    # Generate configs
./subnet-config check       # Diff against system files
./subnet-config copy        # Copy to system locations (requires root)
./subnet-config apply       # Restart affected daemons (requires root)
```

### 2. subnet-authorityd (Authority Daemon)

Runtime daemon that:

- Browses mDNS continuously using DNS-SD meta-query (`_services._dns-sd._udp.local.`)
- Caches discovered services in SQLite (WAL mode)
- Serves REST API exposing the service cache
- Self-advertises as `_subnet-authority._tcp.local` for zero-config discovery

**REST API Endpoints:**

| Endpoint | Description |
|----------|-------------|
| `GET /v1/config` | Authority metadata (zone, prefix, ports) |
| `GET /v1/services` | Full service list (JSON) |
| `GET /v1/services?type=X` | Services filtered by type |
| `GET /v1/services/{instance}` | Single service detail |
| `GET /v1/services/hash` | SHA-256 hash for change detection |

**Key Design:**

- Channel-based architecture: mDNS browser → cache manager → SQLite (dedicated thread)
- Hash computed on cache changes, served from memory for cheap polling
- Graceful shutdown with `CancellationToken`
- Testable: All major components have unit tests

### 3. subnet-client (Client Agent)

**Not yet implemented.** Will provide:

- Authority discovery via mDNS or well-known address
- Service list synchronization
- Local DNS resolver configuration
- Optional local cache for fast lookups

## Project Structure

```
subnet-authority/
├── Cargo.toml                 # Workspace root
├── shared/                    # Shared types library
├── subnet-authorityd/         # Authority daemon (Rust)
├── subnet-config-rs/          # Stub (shell script is real impl)
├── subnet-client/             # Stub
├── subnet-config              # Shell script (actual impl)
├── examples/                  # Example configs
├── systemd/                   # Systemd unit files
└── archnotes/                 # Architecture docs
```

## Documentation

- `archnotes/ARCHITECTURE.md` — Full technical design
- `archnotes/README.md` — Project overview and rationale
- `archnotes/RELATED.md` — Survey of related projects
- `doc/subnet-config-code-explainer.md` — Shell script internals

## Dependencies

### subnet-authorityd

- `tokio` — Async runtime
- `axum` — HTTP server
- `mdns-sd` — mDNS browsing and advertising
- `rusqlite` — SQLite database (bundled)
- `serde` + `toml` — Config parsing
- `tracing` — Structured logging
- `sha2` + `hex` — Cache hashing

## Testing

```bash
# Run all tests
cargo test

# Run daemon tests only
cargo test -p subnet-authorityd

# Build and run with example config
cargo run -p subnet-authorityd -- examples/authorityd.toml
```

## License

(To be determined)

## Contributing

This is a personal project in active development. See `archnotes/ARCHITECTURE.md` for design details.
