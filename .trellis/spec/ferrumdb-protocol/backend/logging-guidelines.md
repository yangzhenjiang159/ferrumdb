# Logging Guidelines — `ferrumdb-protocol`

## Rule: No Logging In This Crate

`ferrumdb-protocol` is a library. `crates/ferrumdb-protocol/src/lib.rs` has
zero `tracing` calls and the `Cargo.toml` does not list `tracing`.

## Why

- Encode/decode is on the hot path of every connection
- Adding `tracing` here would propagate to every consumer
- Errors carry enough context (sequence id, command byte) for callers to log

## Planned (when implementation lands)

| Site | Level | Why |
|------|-------|-----|
| Handshake start | `debug!` | Per-connection event |
| Handshake success | `debug!` | Per-connection event |
| Unknown command | `warn!` | Operator-visible |
| Packet too large | `warn!` | Operator-visible |
| Capability mismatch | `warn!` | Operator-visible |

The hot path (per-packet decode) stays silent.

## Cross-Reference

Same rule as every other library crate. Only `ferrumdb-server` initializes a
subscriber.
