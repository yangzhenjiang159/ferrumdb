# Quality Guidelines — `ferrumdb-server`

## Required Patterns

1. **`#![deny(missing_docs)]`** in `lib.rs` (planned split from `main.rs`).

2. **`tracing_subscriber::fmt::init()` first** in `main()` — already true.

3. **Library + binary split**: business logic in `lib.rs`, `main.rs` is a thin shim.

4. **`anyhow::Result`** in `main()`; **`ServerError`** (thiserror) inside.

5. **One `Session` per connection** with bounded buffers and backpressure.

6. **Graceful shutdown** on `SIGINT` / `SIGTERM`.

7. **`tokio::task::spawn_blocking`** wraps any call into the sync storage stack.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| Hard-coding bind address | Operators need to configure |
| `unwrap()` on network / storage calls | Runtime crash = downtime |
| `anyhow` inside `Session` | Loses structured error info |
| Logging query text at `info!` | PII / secrets |
| `panic!` on per-connection errors | Single bad client kills server |
| Blocking the runtime with sync storage calls | All connections stall |
| Spawning unbounded tasks | DoS risk |

## Testing Requirements

When implementation lands:

- [ ] `Server::bind` succeeds on an available port
- [ ] `Server::run` accepts a connection
- [ ] End-to-end: `mysql CLI` connects, runs `SELECT 1`, gets a row
- [ ] End-to-end: malformed SQL returns an Error packet, connection stays open
- [ ] Graceful shutdown: `SIGINT` during a query; query completes or errors, then server exits 0
- [ ] Backpressure: a slow client doesn't exhaust memory
- [ ] Logging test: `info!` appears for server start, `debug!` for connection events

## Code Review Checklist

- [ ] Module `//!` doc references phase 8
- [ ] `tracing_subscriber::fmt::init()` is the first call in `main()`
- [ ] Library logic lives in `lib.rs`, not `main.rs`
- [ ] No `unwrap()` / `expect()` outside tests
- [ ] Bind address is configurable
- [ ] `cargo test -p ferrumdb-server` passes
