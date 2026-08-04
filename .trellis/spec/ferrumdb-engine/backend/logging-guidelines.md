# Logging Guidelines — `ferrumdb-engine`

## Rule: No Logging In This Crate

`ferrumdb-engine` is a library. The current source
(`crates/ferrumdb-engine/src/engine.rs`) has zero `tracing` calls and the
`Cargo.toml` does not list `tracing`.

## Why

- Trait methods (`insert`, `scan`, etc.) are on the hot path
- This crate has no async runtime; adding `tracing` would propagate to consumers
- Errors carry enough context (`EngineError::TableNotFound(name)`, etc.) for callers to log meaningfully

## Planned (when implementation lands)

| Site | Level | Why |
|------|-------|-----|
| `create_table` start | `debug!` | Pair with success log |
| `create_table` success | `info!` | Operator-visible |
| `insert` / `update` / `delete` | `trace!` | Hot path; off by default |
| `get_by_pk` cache miss | `trace!` | Useful for buffer-pool tuning |
| `commit` success | `info!` | Operator-visible |
| `commit` failure | `error!` | Always visible |
| `Unsupported` returned | `debug!` | Indicates a phase-7 caller hit a phase-9 method |

The body of hot-path methods (`insert`, `get_by_pk`, `scan`) stays silent.

## Cross-Reference

Same rule as every other library crate. Only `ferrumdb-server` initializes a
subscriber; the `ferrumdb-engine` crate logs are emitted in tests and via the
caller (typically `ferrumdb-server` or integration tests).
