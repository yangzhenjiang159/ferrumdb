# Logging Guidelines — `ferrumdb-sql`

## Rule: No Logging In This Crate

`ferrumdb-sql` is a library. `crates/ferrumdb-sql/src/lib.rs` has zero
`tracing` calls and the `Cargo.toml` does not list `tracing`.

## Why

- Every executed statement goes through here
- Adding `tracing` would propagate to consumers
- Errors carry `Span` context — callers can log with line/column

## Planned (when implementation lands)

| Site | Level | Why |
|------|-------|-----|
| `Executor::execute` start | `trace!` | Off by default; useful in profiling |
| `Parser` parse error | `debug!` | Pair with the error returned |
| Unsupported statement | `warn!` | Operator-visible (someone sent a v2 query) |

The parser and executor hot path stays silent.

## Cross-Reference

Same rule as every other library crate. Only `ferrumdb-server` initializes a
subscriber.
