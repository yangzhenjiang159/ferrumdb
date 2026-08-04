# Directory Structure — `ferrumdb-server`

Real layout (2026-07-18):

```
crates/ferrumdb-server/
├── Cargo.toml              # engine + protocol + sql + tokio + tracing + tracing-subscriber + anyhow
└── src/
    └── main.rs             # Binary entry point — calls tracing_subscriber::fmt::init() + tracing::info!
```

`crates/ferrumdb-server/src/main.rs:5-7` is the entire `main()` today.

## Planned (not yet present)

| File | Purpose | Phase |
|------|---------|-------|
| `lib.rs` | Library crate alongside the binary so tests can drive internals | 8 |
| `server.rs` | `Server` struct: bind, accept, spawn per-connection task | 8 |
| `session.rs` | `Session` per-connection state machine | 8 |
| `config.rs` | CLI / env var parsing for bind address, port, log level | 8 |
| `shutdown.rs` | `SIGINT` / `SIGTERM` handling | 8 |

## Conventions

- Even though this is a binary crate, the actual logic lives in a `lib.rs` so unit tests can drive `Server` / `Session` directly
- `main.rs` becomes a thin shim: parse config → init tracing → build `Server` → run
- `Session` is the unit of work — keep it `Send + 'static` so `tokio::spawn` can own it
