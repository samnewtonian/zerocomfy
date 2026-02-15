# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**subnet-authority** — a lightweight service discovery system for self-configuring IPv6 ULA subnets. Bridges mDNS/DNS-SD to devices that can't participate in it (embedded systems, tunneled remote clients). The authority passively browses mDNS, caches discovered services in SQLite, and exposes them via REST, CoAP, and DNS interfaces under a separate zone (not `.local`).

**Status: Design phase.** Architecture docs are in `archnotes/`. No source code exists yet.

## Three Project Segments

1. **`subnet-config`** — One-shot CLI tool. Reads a declarative TOML file and generates native config files for system daemons (`radvd.conf`, systemd-networkd units, dnsmasq, WireGuard). Subcommands: `generate`, `check`, `apply`.

2. **`subnet-authorityd`** + **`subnet-client`** — Runtime daemons. The authority daemon continuously browses mDNS, maintains a SQLite cache (WAL mode), and serves it via REST (`/v1/services`, `/v1/config`), CoAP (with Observe), and DNS (configurable zone like `subnet.example`). The client agent discovers the authority via mDNS, syncs the service list, and configures the local DNS resolver.

3. **`ban-gateway`** — Future BLE/wearable bridge. Out of scope for initial implementation.

## Tech Stack

- **Language:** Rust — Cargo workspace producing three binaries + a shared library crate
- **Key crates:** `mdns-sd` (mDNS browsing), `axum` or `tiny_http` (HTTP), `hickory-dns` (DNS serving), `coap-lite` (CoAP), `rusqlite` (SQLite), `toml`+`serde` (config), `rustls` (TLS), `tracing` (logging), `zbus` (D-Bus for resolver config)

## Planned Build Commands

```bash
cargo build                          # build all workspace members
cargo build -p subnet-config         # build just subnet-config
cargo build -p subnet-authorityd     # build just the authority daemon
cargo build -p subnet-client         # build just the client agent
cargo test                           # run all tests
cargo test -p subnet-authorityd      # test a single crate
```

## Architecture Key Points

- The authority is a **cache**, not the source of truth. The network (mDNS) is the source of truth.
- Setup and runtime are deliberately separated: `subnet-config` handles system config (different privileges, different lifecycle) while `subnet-authorityd` handles continuous discovery.
- Three tiers of participation: passive devices (just avahi), authority-aware devices (client agent), constrained devices (REST/CoAP/DNS API).
- DNS zone is separate from `.local` — e.g., `nas.local` in mDNS becomes `nas.subnet.example` in the authority's zone.
- Failover uses independent mDNS browsing on a standby node with VRRP-like `::1` address takeover — no replication protocol needed.
- Cache change detection uses a hash endpoint (`/v1/services/hash`) so clients can cheaply check staleness before pulling the full list.
- The authority self-advertises as `_subnet-authority._tcp.local` with TXT records containing zone, prefix, and port info for zero-config client bootstrapping.

## Git Commit Policy

Before creating a commit, check `git config commit.gpgsign`. If it is `true`, do **not** run `git commit`. Instead, print the proposed commit message so the user can commit manually with their GPG key.

## Reference Documents

- `archnotes/README.md` — Project overview and design principles
- `archnotes/ARCHITECTURE.md` — Full technical design with config schemas, API specs, and planned project structure
- `archnotes/RELATED.md` — Survey of related projects and prior art (RFC 8766, Consul, OpenThread, etc.)
