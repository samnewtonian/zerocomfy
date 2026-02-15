# subnet-authorityd refactor — 2026-02-15

Code review of the initial MVP identified 8 issues ranging from a broken handler to architectural anti-patterns. This document records each fix, the rationale, and the files affected.

## Fix 1: Broken `get_config` handler

**Files:** `subnet-authorityd/src/api/routes.rs`, `subnet-authorityd/src/main.rs`

The `/v1/config` handler tried to parse `state.config.zone` (a string like `"subnet.example"`) as a `u16` port number. This always failed, silently falling back to 8053 regardless of the actual listen address.

The port is already correctly parsed in `main.rs` (lines 58-62). The fix adds an `api_port: u16` field to `AppState` so the value computed once in `main.rs` is passed directly to the handler, eliminating the broken parse.

```rust
// routes.rs — before
let api_port = state.config.zone.parse::<u16>().unwrap_or(8053);

// routes.rs — after
api_port: state.api_port,
```

## Fix 2: Hash recomputed after every cache command

**Files:** `subnet-authorityd/src/cache_manager.rs`

The cache thread recomputed the SHA-256 hash after every command, including reads (`GetAll`, `GetByType`, `GetOne`) and `Shutdown`. A `GET /v1/services` request triggered a full table scan plus hash computation as a side effect.

The fix moves the `recompute_hash` call into only the mutation arms:

- `Upsert` — only when the result is `Ok(true)` (data actually changed)
- `MarkDead` — on success
- `Maintenance` — on success

A `recompute_hash` closure keeps the logic in one place.

## Fix 3: Hash includes unstable fields

**Files:** `subnet-authorityd/src/cache/hash.rs`

`compute_hash` serialized the full `ServiceEntry` including `last_seen`, `first_seen`, and `ttl`. Since `last_seen` is updated on every mDNS re-resolve, the hash changed constantly even when no real service data changed. This made the `/v1/services/hash` endpoint useless for cheap staleness checks — it would always report a change.

The fix introduces a `HashView` struct that borrows only the stable fields from `ServiceEntry`:

```rust
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
```

A new test (`test_hash_stable_across_timestamp_changes`) verifies that mutating `last_seen`, `first_seen`, and `ttl` does not alter the hash.

## Fix 4: Browser polling with `try_recv` + sleep

**Files:** `subnet-authorityd/src/mdns/browser.rs`

The second `select!` arm used `try_recv()` on each type-specific receiver in a loop, sleeping 100ms between iterations. This is a polling anti-pattern: it wastes CPU cycles when idle and adds up to 100ms latency for every service event.

The fix replaces this with a `FuturesUnordered` of `recv_async()` futures. Each future carries the receiver index and the receiver itself (passed by value — `flume::Receiver` is `Clone`). When a future resolves, the event is processed and a new `recv_async()` future is pushed for the same receiver. `FuturesUnordered::next()` inside `tokio::select!` wakes only when an event arrives — zero polling, zero latency.

```rust
type RecvResult = (usize, flume::Receiver<ServiceEvent>, Result<ServiceEvent, flume::RecvError>);
type RecvFuture = Pin<Box<dyn Future<Output = RecvResult> + Send>>;

fn make_recv_future(idx: usize, rx: flume::Receiver<ServiceEvent>) -> RecvFuture {
    Box::pin(async move {
        let result = rx.recv_async().await;
        (idx, rx, result)
    })
}
```

The `Box::pin` erases the async block type so all futures can coexist in the same `FuturesUnordered`. The receiver is moved into and back out of each future to satisfy the borrow checker without requiring `Arc` or unsafe code.

## Fix 5: `BrowserEvent` defined in wrong module

**Files:** `subnet-authorityd/src/mdns/browser.rs`, `subnet-authorityd/src/cache_manager.rs`

`BrowserEvent` was defined in `cache_manager.rs` but represents browser output. The browser module imported it from `cache_manager`, creating a backwards dependency (producer depends on consumer).

The fix moves `BrowserEvent` to `mdns/browser.rs` where it logically belongs. `cache_manager.rs` re-exports it with `pub use crate::mdns::browser::BrowserEvent` so downstream code continues to compile without import changes.

## Fix 6: Maintenance interval too frequent

**Files:** `subnet-authorityd/src/config.rs`, `subnet-authorityd/src/cache_manager.rs`, `examples/authorityd.toml`

Maintenance (mark_stale + prune) ran every `browse_interval_secs` (30s). The staleness threshold is 300s and the prune threshold is 3600s — maintenance was running 10-120x more often than necessary.

The fix adds a dedicated `maintenance_interval_secs: u64` field to `CacheConfig` with a default of 60 seconds. The cache manager loop uses this value instead of `browse_interval_secs`.

```toml
[cache]
maintenance_interval_secs = 60
```

## Fix 7: Fragile SQL-based change detection

**Files:** `subnet-authorityd/src/cache/db.rs`

`upsert_service` detected changes by comparing a SQL `||`-concatenated string (built in SQLite) against a Rust `format!` string. The field order and serialization format were duplicated across two languages. Any mismatch — different quoting, different JSON whitespace, reordered fields — would silently break change detection.

The fix selects the existing row as a full `ServiceEntry` via the existing `row_to_entry` helper, then compares meaningful fields in Rust:

```rust
fn service_data_changed(old: &ServiceEntry, new: &ServiceEntry) -> bool {
    old.hostname != new.hostname
        || old.addresses != new.addresses
        || old.port != new.port
        || old.txt != new.txt
        || old.ttl != new.ttl
        || old.alive != new.alive
        || old.service_type != new.service_type
}
```

This is explicit about which fields matter for change detection and keeps the logic in one language.

## Fix 8: `compute_hash` clones entire service list

**Files:** `subnet-authorityd/src/cache/hash.rs`

`compute_hash` called `services.to_vec()` to sort a clone of every `ServiceEntry`. Each entry contains heap-allocated `String`s, `Vec`s, and `HashMap`s, making the clone expensive for large service lists.

The fix sorts a `Vec<usize>` of indices by `instance_name`, then iterates in sorted order to build the `HashView` references. Combined with fix 3, this means `compute_hash` now does zero heap allocation beyond the index vector and the final JSON string.

```rust
let mut indices: Vec<usize> = (0..services.len()).collect();
indices.sort_by(|&a, &b| services[a].instance_name.cmp(&services[b].instance_name));
```

## Files modified (summary)

| File | Fixes |
|------|-------|
| `subnet-authorityd/src/api/routes.rs` | #1 |
| `subnet-authorityd/src/main.rs` | #1 |
| `subnet-authorityd/src/cache_manager.rs` | #2, #5, #6 |
| `subnet-authorityd/src/cache/hash.rs` | #3, #8 |
| `subnet-authorityd/src/mdns/browser.rs` | #4, #5 |
| `subnet-authorityd/src/cache/db.rs` | #7 |
| `subnet-authorityd/src/config.rs` | #6 |
| `examples/authorityd.toml` | #6 |

## Verification

- `cargo build` — clean (2 pre-existing dead-code warnings unrelated to these changes)
- `cargo test -p subnet-authorityd` — 7/7 tests pass
- `test_hash_deterministic` — still passes after fix 3 changed what's hashed
- `test_hash_stable_across_timestamp_changes` — new test validating fix 3
- `test_upsert_detects_changes` — still passes after fix 7 changed detection method
