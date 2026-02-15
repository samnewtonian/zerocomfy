# Related Projects & Prior Art

A survey of existing projects, standards, and tools with overlapping design goals. Organized by relevance to the two main components: the subnet authority (mDNS cache/proxy + service API) and the future BLE/wearables gateway.

---

## mDNS Proxy & Service Discovery

### RFC 8766 — Discovery Proxy for Multicast DNS-Based Service Discovery

The IETF standard that formally specifies the pattern this project implements. A Discovery Proxy browses the local link via mDNS and serves the discovered records over unicast DNS to remote clients. It defines behavior for standard queries, long-lived queries (LLQ), and DNS Push Notifications (RFC 8765) for subscription-based updates.

**Relevance:** This is the closest formal standard to what the authority does. The main difference is that RFC 8766 only defines a DNS interface, while this project also provides REST and CoAP. RFC 8766 is also designed for multi-link enterprise networks with DNS delegation, whereas this project targets a single-link home lab.

- Spec: <https://datatracker.ietf.org/doc/rfc8766/>
- Requirements doc: <https://datatracker.ietf.org/doc/rfc7558/>

---

### Apple mDNSResponder

Apple's reference implementation of mDNS, DNS-SD, the Discovery Proxy (RFC 8766), DNS Push (RFC 8765), and the Service Registration Protocol (SRP). This is the most complete open-source implementation of the mDNS-to-unicast-DNS bridge pattern. It also includes an SRP server that accepts service registrations from constrained devices (used by Thread/OpenThread) and re-advertises them via mDNS.

**Relevance:** The SRP component is architecturally similar to the CoAP registration endpoint in this project — constrained devices that can't do mDNS register with a proxy that advertises on their behalf. However, mDNSResponder is a large, complex C codebase tightly coupled to Apple's ecosystem and difficult to extract components from.

- Source: <https://github.com/apple-oss-distributions/mDNSResponder>

---

### mkuron/mdns-discovery-proxy

A minimal Python implementation of RFC 8766 using the `zeroconf` library for mDNS browsing and Twisted for serving unicast DNS. The author explicitly notes it is not fully standards-compliant but is functional with macOS and iOS clients. Created because the Apple implementation is hard to run on Linux and the only other option (ohybridproxy) only runs on OpenWRT.

**Relevance:** Small, readable reference for the core mDNS→DNS proxy pattern. Demonstrates how little code is actually needed for the basic functionality. Good for understanding the flow without wading through Apple's codebase.

- Source: <https://github.com/mkuron/mdns-discovery-proxy>

---

### CoreDNS with mDNS Plugin (openshift/coredns-mdns)

A CoreDNS external plugin that reads mDNS records from the local network and serves them as standard DNS records under a configurable domain. For example, `nas.local` becomes `nas.example.com`. Supports filtering by service name and binding to specific network interfaces.

**Relevance:** Very close to the DNS interface component of the authority. The original CoreDNS feature request describes exactly this project's motivation: making mDNS-advertised hosts available to applications that don't support mDNS. Practical limitations noted by users include race conditions with multiple network interfaces and the need to manually configure avahi on each host to publish workstation services. The authority's continuous browsing approach avoids some of these issues.

- Plugin source: <https://github.com/openshift/coredns-mdns>
- CoreDNS: <https://coredns.io/>
- Original feature request: <https://github.com/coredns/coredns/issues/317>
- Avahi/Bonjour plugin discussion: <https://github.com/coredns/coredns/issues/1519>

---

### OpenThread Border Router (OTBR)

Google's open-source Thread border router implementation. An OTBR connects a Thread 802.15.4 mesh network to IP-based infrastructure (WiFi/Ethernet), providing bidirectional IPv6 connectivity and service discovery. Thread devices register services with the border router using SRP, and the border router re-advertises them via mDNS on the infrastructure link. The OTBR also handles Router Advertisements for the Thread mesh.

**Relevance:** The closest architectural analog to this project's full design. The OTBR bridges constrained devices (Thread nodes that can't do mDNS) into the wider network's service discovery — exactly what the authority does for embedded/tunneled devices. Key differences: Thread uses 802.15.4 radio (not BLE or WiFi), the OTBR is tightly coupled to the Thread protocol stack, and it uses SRP rather than REST/CoAP for service registration. The SRP→mDNS proxy pattern is nevertheless a strong reference.

- Project site: <https://openthread.io/guides/border-router>
- Codelab (service discovery): <https://openthread.io/codelabs/openthread-border-router>
- Home Assistant integration: <https://www.home-assistant.io/integrations/thread/>

---

### HashiCorp Consul

A full-featured service networking solution providing service discovery (via DNS and HTTP API), health checking, service mesh with mTLS, multi-datacenter support, and key-value storage. Consul uses Raft consensus for its server cluster and runs lightweight agents on each node. Services must explicitly register themselves.

**Relevance:** Consul solves the same fundamental problem (how do services find each other?) but at datacenter scale with enterprise features. It's massive overkill for a home lab, but several design patterns are worth studying: the dual DNS + HTTP interface for service lookup, the health check model, and the agent-per-node architecture. The key philosophical difference is that Consul requires explicit registration while this project auto-populates from mDNS.

- Site: <https://developer.hashicorp.com/consul>
- Source: <https://github.com/hashicorp/consul>
- Service discovery docs: <https://developer.hashicorp.com/consul/docs/use-case/service-discovery>

---

### libp2p mDNS Discovery

The libp2p networking stack (used by IPFS, Filecoin, and others) includes an mDNS-based peer discovery module for finding peers on the local network with zero configuration. It defines a `_p2p._udp.local` service type and encodes peer identity (public key hash) in DNS-SD TXT records.

**Relevance:** Not directly applicable architecturally, but a good reference for how another project layered identity and metadata onto DNS-SD records. The encoding of peer identity in TXT records is similar to how the authority might encode device ownership or authentication info for the future BLE gateway work.

- Spec PR: <https://github.com/libp2p/specs/pull/80/files>

---

### mjansson/mdns (C library)

A public-domain, single-header mDNS/DNS-SD library in C. Covers discovery, query, service response, and announcements. Supports both IPv4 and IPv6. Designed as a minimal, embeddable library.

**Relevance:** If any component of the project needs to be written in C (e.g., a very constrained embedded client), this is a clean, dependency-free starting point for mDNS functionality.

- Source: <https://github.com/mjansson/mdns>

---

## Rust mDNS Libraries

### mdns-sd

The primary candidate for the authority's mDNS browsing layer. A pure Rust mDNS-SD implementation supporting both querier (client) and responder (server) roles. Uses a dedicated thread with `flume` channels for communication, avoiding a dependency on any specific async runtime. Tested for compatibility with Avahi (Linux), dns-sd (macOS), and Bonjour (iOS).

- Source: <https://github.com/keepsimple1/mdns-sd>
- Crate: <https://crates.io/crates/mdns-sd>
- Docs: <https://docs.rs/mdns-sd>

### simple-mdns

A pure Rust mDNS/DNS-SD implementation with both sync and async service discovery modules. Includes a standalone responder for environments without an existing mDNS server. Lighter weight than `mdns-sd` but less feature-complete.

- Crate: <https://crates.io/crates/simple-mdns>
- Docs: <https://docs.rs/simple-mdns>

### libmdns

A pure Rust mDNS/DNS-SD responder built on tokio, originally from the Spotify librespot project. Responder-only (no querier), so useful as a reference for service advertisement but not for the browsing/caching side.

- Source: <https://github.com/librespot-org/libmdns>

---

## BLE Gateways & Body Area Networks

*These are relevant to the future wearables gateway component, not the current authority implementation.*

### OpenMQTTGateway / Theengs

An open-source firmware for ESP32 (and ESP8266) that acts as a multi-protocol gateway, bridging BLE, 433MHz RF, infrared, and LoRa to MQTT. The BLE component scans for nearby BLE sensors, decodes their advertisements using the Theengs decoder library (supporting 100+ device types), and publishes the data to an MQTT broker. Theengs Gateway extends this to run on Raspberry Pi and desktop systems.

**Relevance:** The closest open-source project to the BLE gateway concept. It bridges BLE devices to a network protocol (MQTT), supports a wide range of sensors, and runs on ESP32. Key differences from this project's future gateway: OMG uses MQTT rather than mDNS/DNS-SD + CoAP, focuses on passive BLE scanning (broadcast advertisements) rather than GATT connections to paired devices, and has no concept of per-person device ownership or body area networks.

- Source: <https://github.com/1technophile/OpenMQTTGateway>
- Docs: <https://docs.openmqttgateway.com/>
- Theengs Gateway: <https://gateway.theengs.io/>

---

### DusunIoT BLE Gateways (Commercial)

Commercial BLE-to-IP gateway hardware using Silicon Labs EFR32BG24 SoCs. Notably, they produce a portable model specifically designed for wearable devices like CGMs (continuous glucose monitors), with WiFi and LTE uplinks and battery management. Also offer Rockchip-based models capable of cloud-free local hub operation.

**Relevance:** Validates the gateway form factor and use case. The portable CGM gateway is essentially a commercial version of the per-person BLE gateway node envisioned for this project. Closed-source, but useful as a hardware reference.

- Product page: <https://www.dusuniot.com/landing-pages/bluetooth-gateway/>

---

### IPv6 over BLE Mesh (6LoWPAN)

The IETF's 6LoWPAN-over-BLE work (RFC 7668) and the Bluetooth IPSP (Internet Protocol Support Profile) enable direct IPv6 connectivity for BLE devices without a gateway proxy. The uwbiot/IPv6OverBluetoothLowEnergyMesh project demonstrates multi-hop IPv6 packet transfer over a BLE mesh.

**Relevance:** An alternative architectural approach where BLE devices are directly IPv6-addressable rather than proxied through a gateway. More elegant in theory, but impractical for very constrained implants and wearables that lack the resources for an IPv6 stack. The gateway proxy model is more appropriate for the target devices. However, 6LoWPAN-over-BLE could be interesting for future, more capable wearable hardware.

- RFC 7668 (IPv6 over BLE): <https://datatracker.ietf.org/doc/rfc7668/>
- Demo project: <https://github.com/uwbiot/IPv6OverBluetoothLowEnergyMesh>

---

### IEEE 802.15.6 / WBAN Standards & Research

The IEEE 802.15.6 standard defines physical and MAC layer specifications for Wireless Body Area Networks. ETSI's SmartBAN technical committee extends this with work on interoperability between BANs and wider IoT infrastructure, including coexistence with other wireless technologies and gateway integration patterns.

**Relevance:** The formal standards work for body-worn and implanted device communication. Defines the BAN architecture with Body Sensor Units (BSUs) and a Body Central Unit (BCU) — mapping to this project's wearable devices and gateway node respectively. The academic literature is heavy on physical layer and MAC optimization but light on the network integration side (how BAN data reaches LAN services), which is exactly the gap the future gateway component would fill.

- ETSI SmartBAN: <https://www.etsi.org/technologies/smart-body-area-networks>
- Wikipedia overview: <https://en.wikipedia.org/wiki/Body_area_network>
- Stanford gateway paper: <https://iot.stanford.edu/pubs/zachariah-gateway-hotmobile15.pdf>

---

### ESP-IDF mDNS Component

Espressif's ESP-IDF framework includes a built-in mDNS component for ESP32 devices, supporting hostname registration, service advertisement, and service discovery. This means ESP32 nodes on the subnet could potentially participate in mDNS directly, reducing their dependence on the authority for service discovery (though they would still benefit from the cache for faster lookups and would still need it for cross-link access).

**Relevance:** Important for understanding what the ESP32 platform can do natively. An ESP32 with WiFi can run mDNS and advertise services itself. The authority is most valuable for ESP32 devices on constrained links (BLE-only, no WiFi) or for avoiding the cold-query latency issue.

- ESP-IDF mDNS docs: <https://espressif.github.io/esp32-c3-book-en/chapter_8/8.2/8.2.4.html>
- Component source: <https://github.com/espressif/esp-idf/tree/master/components/mdns>

---

## Summary Table

| Project | Overlap with Authority | Overlap with Gateway | Language | Scale Target |
|---|---|---|---|---|
| RFC 8766 / Discovery Proxy | High — same mDNS→unicast pattern | None | N/A (spec) | Enterprise multi-link |
| Apple mDNSResponder | High — reference implementation | Low (SRP for Thread) | C | Apple ecosystem |
| mkuron/mdns-discovery-proxy | High — minimal Python impl | None | Python | Home network |
| CoreDNS + mDNS plugin | Medium — DNS interface only | None | Go | Kubernetes / home |
| OpenThread Border Router | High — same bridge pattern for constrained devices | Low (802.15.4, not BLE) | C/C++ | IoT mesh networks |
| HashiCorp Consul | Medium — service discovery concepts | None | Go | Datacenter / enterprise |
| OpenMQTTGateway | Low — different transport (MQTT) | High — BLE scanning + bridging | C++ (Arduino) | Home automation |
| DusunIoT gateways | None | High — commercial BLE gateway hardware | N/A (commercial) | Wearables / medical |
| 6LoWPAN over BLE | Low | Medium — alternative to gateway proxy | C | Research / IoT |
| IEEE 802.15.6 / WBAN | None | Medium — formal BAN standards | N/A (spec) | Medical / research |

---

## Key Takeaway

No existing project combines all of these pieces: a lightweight mDNS cache/proxy authority that also handles ULA prefix advertisement and serves as a bridge for constrained devices, paired with multiple consumption interfaces (REST, CoAP, DNS). The closest is the OpenThread Border Router, but it is locked to the Thread protocol stack and 802.15.4 radio. This project builds the same pattern using standard WiFi/Ethernet + mDNS, generalized for a mixed network of servers, workstations, embedded devices, and (eventually) BLE wearables.
