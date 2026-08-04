# Logging Guidelines — `ferrumdb-txn`

## Rule: No Logging In This Crate

`ferrumdb-txn` is a library. `crates/ferrumdb-txn/src/lib.rs` has zero
`tracing` calls and the `Cargo.toml` does not list `tracing`.

## Why

- `commit` / `rollback` are on the hot path
- This crate has no async runtime; adding `tracing` would propagate to consumers
- Errors carry `TransactionId` context — callers can log meaningfully

## Planned (when implementation lands)

| Site | Level | Why |
|------|-------|-----|
| `Transaction::begin` | `debug!` | Operator-visible during debugging |
| `Transaction::commit` start | `debug!` | Pair with the success/failure log |
| `Transaction::commit` success | `info!` | Operator-visible |
| `Transaction::commit` failure | `error!` | Always visible |
| `Transaction::rollback` | `info!` | Operator-visible |
| `UndoChainBroken` detected | `error!` | Always visible |
| `ReadView` snapshot taken | `trace!` | Off by default; useful for snapshot debugging |

The commit/rollback body itself stays silent except for the start/end markers
listed above. The version-chain walk is `trace!` only.

## Cross-Reference

Same rule as every other library crate. Only `ferrumdb-server` initializes a
subscriber.
