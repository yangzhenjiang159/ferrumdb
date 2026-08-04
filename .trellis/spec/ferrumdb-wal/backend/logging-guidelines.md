# Logging Guidelines — `ferrumdb-wal`

## Rule: No Logging In This Crate

`ferrumdb-wal` is a library. `crates/ferrumdb-wal/src/lib.rs` has zero
`tracing` calls and the `Cargo.toml` does not list `tracing`.

## Why

- `Wal::append` is on the recovery hot path
- Adding `tracing` would propagate to consumers
- Errors carry `lsn` + `page_id` context — callers can log meaningfully

## Planned (when implementation lands)

| Site | Level | Why |
|------|-------|-----|
| `Wal::open` success / failure | `info!` / `error!` | Operator-visible |
| `Wal::append` success | `trace!` | Hot path; off by default |
| `checkpoint` write | `info!` | Operator-visible |
| `RecordCrcMismatch` detected during `recover` | `error!` | Always visible |
| `recover` complete | `info!` | Operator-visible |
| `LsnExhausted` | `error!` | Always visible (data integrity threat) |

The hot path (`Wal::append` body) stays silent.

## Cross-Reference

Same rule as every other library crate. Only `ferrumdb-server` initializes a
subscriber.
