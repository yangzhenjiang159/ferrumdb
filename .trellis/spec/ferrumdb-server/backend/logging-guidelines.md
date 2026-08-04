# Logging Guidelines — `ferrumdb-server`

This is the **only** crate that initializes a tracing subscriber. It is also
the only place where `info!` / `warn!` / `error!` are appropriate as defaults.

## Initialisation

Already in `crates/ferrumdb-server/src/main.rs:6`:

```rust
tracing_subscriber::fmt::init();
```

This uses the default `EnvFilter` (env var `RUST_LOG`). Consider switching to
`tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| ...)`
so operators can tune levels without recompiling.

## Level Conventions

| Site | Level | Notes |
|------|-------|-------|
| Server start | `info!` | Operator-visible |
| Server bind success | `info!` | Includes bind address |
| Server shutdown start | `info!` | Operator-visible |
| Per-connection accept | `debug!` | One log per connection |
| Per-connection close | `debug!` | Pair with accept |
| `COM_QUERY` received | `trace!` | Hot path |
| Per-query latency | `debug!` | Aggregated; not per-query |
| `WARN` level errors | `warn!` | Recoverable but operator-relevant |
| Connection drop / parse error | `warn!` | |
| Critical / unrecoverable | `error!` | |

## What NOT to Log

- Query text at `info!` (may contain PII or secrets; use `debug!` only)
- Full request payloads (memory bloat in logs)
- Per-packet traces (use `trace!` instead — off by default)

## Cross-Reference

Every other crate in the workspace follows the "library, no logging" rule —
see `.trellis/spec/ferrumdb-page/backend/logging-guidelines.md` for the
canonical statement.

## Subscriber Lifecycle

- Initialise once in `main()` before any logging can happen
- Never re-initialise in tests; pass a test subscriber explicitly if needed
- On graceful shutdown, the subscriber is dropped with the process — no
  explicit flush required for `fmt::init()`
