# subnet-authority

A lightweight service discovery system for self-configuring IPv6 ULA subnets. Bridges the gap between mDNS/DNS-SD (which works great on a local link) and devices that can't participate in it — embedded systems, tunneled-in remote clients, and anything that benefits from a persistent, queryable service directory.

## How It Works

```
┌────────────── IPv6 ULA Subnet ──────────────┐
│                                              │
│  ┌───────────┐         ┌──────────┐         │
│  │ Authority  │◀─mDNS─▶│  Server  │         │
│  │ (::1)     │         │ + avahi  │         │
│  │           │         ├──────────┤         │
│  │ cache/API │◀─sync──▶│Workstation         │
│  │ radvd     │         │ + agent  │         │
│  └─────┬─────┘         └──────────┘         │
│        │                                     │
│        │  REST / CoAP / DNS                  │
│        ▼                                     │
│  ┌──────────┐      ┌─────────────┐          │
│  │ ESP32    │      │ Remote node │          │
│  │ (no avahi)│      │ (WireGuard) │          │
│  └──────────┘      └─────────────┘          │
└──────────────────────────────────────────────┘
```

1. The authority advertises a ULA prefix via Router Advertisements. Nodes auto-configure addresses with SLAAC.
2. Nodes advertise services using standard avahi/Bonjour service files. No project-specific configuration needed just to participate.
3. The authority continuously browses mDNS, caching every service it discovers. It also advertises itself via mDNS so client agents can find it.
4. Workstations and servers can run an optional **client agent** that discovers the authority, syncs the service list, and configures the local DNS resolver to forward the authority's zone (e.g. `subnet.example`) — giving transparent name resolution without touching `.local`.
5. Constrained or remote devices query the authority's REST, CoAP, or DNS interface directly.

## Project Segments

The project is split into three parts with distinct responsibilities:

### `subnet-config` — System Setup Utility

A one-shot tool that reads a small declarative TOML file and generates native config files for the system daemons that make a machine act as the authority: `radvd.conf`, systemd-networkd units, optional dnsmasq and WireGuard configs. Translates *what you want* into *how to configure it*, without the authority daemon needing to manage these daemons at runtime.

```
subnet-config generate    # write config files
subnet-config check       # diff against existing, report changes
subnet-config apply       # generate + restart affected services
```

### `subnet-authorityd` + `subnet-client` — Runtime Discovery

**`subnet-authorityd`** is the authority daemon. It continuously browses mDNS, maintains a SQLite cache of all discovered services, and exposes the cache via:

```
GET /v1/config                    → authority metadata (zone, prefix, ports)
GET /v1/services                  → all discovered services
GET /v1/services?type=_http._tcp  → filter by type
GET /v1/services/hash             → cache hash for cheap change detection
```

Plus CoAP (with Observe) for constrained devices and a DNS interface under a configurable zone — `nas.local` in mDNS becomes `nas.subnet.example` in the authority's zone. The `.local` namespace is left to mDNS.

**`subnet-client`** is an optional agent for workstations and servers. It discovers the authority via mDNS, syncs the service list, and configures the local resolver so applications can transparently resolve `nas.subnet.example` without knowing about the project.

### `ban-gateway` — BLE/Wearable Bridge *(Future)*

A separate daemon that bridges BLE wearables and implants to the subnet through per-person gateway nodes. Builds on the authority's service discovery infrastructure. Out of scope for initial implementation.

## Design Principles

- **Separate setup from runtime.** `subnet-config` handles one-time system configuration. `subnet-authorityd` handles continuous discovery. Different lifecycles, different privileges.
- **Lean on existing standards.** SLAAC, mDNS/DNS-SD, DNS. Generate config for established daemons rather than reimplementing them.
- **Three tiers of participation.** Passive devices just use avahi. Workstations run the client agent. Constrained devices query the API. All coexist.
- **Own namespace, don't pollute `.local`.** The authority serves a separate DNS zone. Clients discover it automatically.
- **Generated config is auditable.** Every file `subnet-config` produces is standard, readable, and hand-editable.
- **Simple over complete.** This is a home-lab tool, not enterprise infrastructure.

## Tech Stack

- **Language:** Rust — produces three binaries (`subnet-config`, `subnet-authorityd`, `subnet-client`) from a Cargo workspace
- **mDNS:** `mdns-sd` crate (pure Rust, no avahi dependency on the authority itself)
- **Storage:** SQLite via `rusqlite`
- **Config:** TOML

## Status

Design phase. See [ARCHITECTURE.md](ARCHITECTURE.md) for the full technical design and [RELATED.md](RELATED.md) for a survey of related projects.
