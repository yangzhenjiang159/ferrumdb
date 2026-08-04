# Logging Guidelines — `ferrumdb-buffer`

## Rule: No Logging In This Crate

`ferrumdb-buffer` is a library. `crates/ferrumdb-buffer/src/lib.rs` has zero
`tracing` calls and the `Cargo.toml` does not list `tracing`.

## Why

- `fetch_page` / `flush_all` are on the hot path
- The crate has no async runtime; adding `tracing` would propagate to consumers
- Errors carry `PageId` context — callers can log meaningfully

## Planned (when implementation lands)

| Site | Level | Why |
|------|-------|-----|
| `BufferPool::open` / `create` success | `info!` | Operator-visible event |
| `BufferPool::flush_all` start / end | `info!` | Operator-visible event |
| `evict_lru` flush a dirty page | `debug!` | Pair with frame id |
| `PoolFull` returned | `warn!` | Operator-relevant (caller likely needs more frames) |
| `BufferPoolSource::read_page` cache miss | `trace!` | Useful for cache-hit ratio profiling |

The hot path (`fetch_page` cache hit) stays silent.

## Cross-Reference

Same rule as every other library crate. Only `ferrumdb-server` initializes a
subscriber.
