# Logging Guidelines — `ferrumdb-space`

## Rule: No Logging In This Crate

`ferrumdb-space` is a library. `crates/ferrumdb-space/src/lib.rs` has zero
`tracing` calls and the `Cargo.toml` does not list `tracing`.

## Why

- Page reads/writes are on the hot path
- This crate has no `tracing` dependency; adding one would propagate to consumers
- Errors carry `PageId` context — callers can log meaningfully

## Planned (when implementation lands)

| Site | Level | Why |
|------|-------|-----|
| `Space::open` success | `info!` | Operator-visible event |
| `Space::open` failure | `error!` | Operator-visible event |
| `Space::allocate_page` extend file | `debug!` | Only interesting when debugging space pressure |
| `Space::free_page` | `trace!` | Hot path; off by default |

Even with these added, the page-read/write hot path stays silent.

## Cross-Reference

Same rule as every other library crate. Only `ferrumdb-server` initializes a
subscriber.
