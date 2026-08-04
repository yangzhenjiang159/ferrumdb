# Logging Guidelines — `ferrumdb-page`

## Rule: This Crate Does Not Log

`ferrumdb-page` is a pure CPU library. It performs no I/O and has no async
runtime. There are zero `tracing::` or `log::` calls anywhere in
`crates/ferrumdb-page/src/`. Adding logging here is **forbidden**.

### Why

- This crate is on the hot path of every read/write (`Page::from_bytes` is
  called per page by every higher layer)
- Logging would couple `ferrumdb-page` to `tracing` and break its position as
  the lowest layer (currently it has no `tracing` dependency in `Cargo.toml`)
- `PageError` variants already carry enough information for callers to log at
  the layer where context exists (`ferrumdb-wal` during recovery,
  `ferrumdb-buffer` during eviction)

### When something goes wrong

Propagate the `PageError` to the caller. The caller decides whether to log
based on what it was trying to do:

| Caller | Logging policy |
|--------|----------------|
| `ferrumdb-wal` during replay | `error!("replay failed at lsn {}: {}", lsn, e)` |
| `ferrumdb-buffer` on eviction flush | `warn!("evicting dirty page {}: {}", page_id, e)` |
| `ferrumdb-engine` in `get_by_pk` | propagate as `EngineError::Internal(e.to_string())` |

## Cross-Reference

The only crate that initialises a subscriber today is `ferrumdb-server` —
see `.trellis/spec/ferrumdb-server/backend/logging-guidelines.md`. All other
crates follow this "library, no logging" rule.
