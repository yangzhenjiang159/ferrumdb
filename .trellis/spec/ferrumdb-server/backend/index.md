# Backend Development Guidelines — `ferrumdb-server`

> TCP server entry point: accept connections, decode MySQL wire protocol, dispatch SQL, encode responses.

---

## Overview

`ferrumdb-server` is the only **binary** crate in the workspace
(`[[bin]] name = "ferrumdb-server"`, `path = "src/main.rs"`). It owns:

- `tokio` async runtime for accepting connections
- `tracing` initialisation (`tracing_subscriber::fmt::init()`)
- `anyhow` for `main()`-level error reporting
- Per-connection `Session` struct (decoded protocol, executor, response writer)

Today the crate contains a `main()` placeholder in
`crates/ferrumdb-server/src/main.rs`:

```rust
fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("FerrumDB server — not implemented yet (see docs/plan.md phase 8)");
}
```

Real implementation lands in phase 8 per `docs/plan.md`.

The crate depends on `ferrumdb-engine` (for `StorageEngine`), `ferrumdb-protocol`
(for wire format), `ferrumdb-sql` (for parsing + execution), plus `tokio`,
`tracing`, `tracing-subscriber`, `anyhow`. It is the top of the dependency
stack — see `docs/architecture.md`.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Real binary + planned files | Filled |
| [Database Guidelines](./database-guidelines.md) | Connection lifecycle, session model | Filled |
| [Error Handling](./error-handling.md) | `anyhow` in main, structured errors inside | Filled |
| [Logging Guidelines](./logging-guidelines.md) | **Only crate that logs** | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Async hygiene, graceful shutdown, tests | Filled |

---

## Pre-Development Checklist

- [ ] `tracing_subscriber::fmt::init()` is the very first call in `main()` (already true today)
- [ ] One `Session` per connection; sessions share `Arc<StorageEngine>` (read) and have `&mut dyn StorageEngine` for writes (serialised)
- [ ] Graceful shutdown on `SIGINT` / `SIGTERM`
- [ ] Backpressure: bound the number of in-flight queries per connection
- [ ] Bind address from env var or CLI flag, not hard-coded
- [ ] No `unsafe`

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references phase 8
- [ ] `anyhow` is used only in `main()`; libraries above use their own error types
- [ ] Per-connection logs include the connection's peer address (no PII beyond that)
- [ ] Graceful shutdown test: send `SIGINT`, server stops accepting and drains in-flight requests
- [ ] End-to-end test: `mysql` CLI connects, runs `SELECT 1`, disconnects cleanly
