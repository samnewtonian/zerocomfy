# subnet-config

A POSIX shell script for generating system configuration files for an IPv6 ULA subnet authority.

## Quick Start

```bash
# 1. Edit subnet.conf with your network parameters
vi subnet.conf

# 2. Generate configuration files
./subnet-config generate

# 3. Check differences with existing system files
./subnet-config check

# 4. Copy generated files to system locations (requires root)
sudo ./subnet-config copy

# 5. Restart affected daemons (requires root)
sudo ./subnet-config apply
```

## Subcommands

- **`generate`** — Parse `subnet.conf` and generate config files to the current directory (or `-o <dir>`)
- **`check`** — Compare generated files with existing system files and show diffs
- **`copy`** — Copy generated files to system locations (`/etc/radvd.conf`, `/etc/systemd/network/`, `/etc/dnsmasq.d/`)
- **`apply`** — Restart affected daemons (`radvd`, `systemd-networkd`, `dnsmasq`)

## Options

- **`-c <path>`** — Specify config file (default: `subnet.conf` in script directory)
- **`-o <dir>`** — Specify output directory for generated files (default: current directory)

## Configuration Format

See `subnet.conf` for an example. Required fields:

```ini
[authority]
interface = eth0 wlan0          # Space-separated list
prefix = fd00:1234:5678:1::/64
address = fd00:1234:5678:1::1/64
zone = subnet.example
```

## Generated Files

| File | Purpose |
|------|---------|
| `radvd.conf` | Router Advertisement daemon config (IPv6 prefix announcements) |
| `50-subnet-authority-<iface>.network` | systemd-networkd units (one per interface) |
| `subnet-authority.conf` | dnsmasq config (DNS authoritative zone) |

## Typical Workflow

```bash
# Edit config
vi subnet.conf

# Generate and preview
./subnet-config generate
cat radvd.conf

# Check what would change
./subnet-config check

# Deploy (requires root)
sudo ./subnet-config copy
sudo ./subnet-config apply
```

## POSIX Compatibility

The script uses POSIX sh (`#!/bin/sh`) and is compatible with bash, zsh, dash, and other POSIX shells. No bashisms are used.

## See Also

- `archnotes/ARCHITECTURE.md` — Full technical design
- `archnotes/README.md` — Project overview
