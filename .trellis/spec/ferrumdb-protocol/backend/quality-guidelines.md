# Quality Guidelines — `ferrumdb-protocol`

## Required Patterns

1. **`#![deny(missing_docs)]`** — set in `crates/ferrumdb-protocol/src/lib.rs:11`.

2. **Module `//!` doc** in standard format (Chinese responsibilities + phase 8 reference).

3. **Error type in `error.rs`**, `thiserror` derive.

4. **Zero-copy**: packet payloads use `bytes::Bytes` slices, never `String` or `Vec<u8>` clones.

5. **MySQL-compatible error codes** for every `ProtocolError` variant (see error-handling.md).

6. **Length-prefixed framing**: every read first reads the 4-byte header, then exactly the declared payload length.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| `String` for packet payloads | Defeats zero-copy |
| `unsafe` | Not needed |
| Charset negotiation in v1 | Per plan.md 阶段 8 |
| Prepared-statement support in v1 | Deferred per plan.md |
| Returning malformed packets to the client | Surface as `ProtocolError` first |

## Testing Requirements

When implementation lands:

- [ ] Packet frame round-trip: encode → decode → assert equal bytes
- [ ] Handshake packet satisfies MySQL `mysql CLI` parsing
- [ ] OK / Error / ResultSet packets round-trip
- [ ] Truncated payload returns `Truncated(seq)`
- [ ] Oversized payload returns `PacketTooLarge(len)`
- [ ] Unknown command byte returns `UnknownCommand(b)`
- [ ] `mysql CLI` integration test: connect, send `SELECT 1`, get response

## Code Review Checklist

- [ ] Module `//!` doc references phase 8
- [ ] No new external dep beyond `bytes` (already in workspace)
- [ ] All `pub` items have `///` doc comments
- [ ] MySQL error code mapping documented per variant
- [ ] Charset is `utf8mb4` only (or whatever is documented)
- [ ] `cargo test -p ferrumdb-protocol` passes
