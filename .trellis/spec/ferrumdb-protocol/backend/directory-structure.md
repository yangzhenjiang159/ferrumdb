# Directory Structure — `ferrumdb-protocol`

Real layout (2026-07-18):

```
crates/ferrumdb-protocol/
├── Cargo.toml              # bytes + thiserror
└── src/
    └── lib.rs              # Module doc + #![deny(missing_docs)] + crate_compiles smoke test
```

`crates/ferrumdb-protocol/src/lib.rs:11-13` defines the placeholder test.

## Planned (not yet present)

| File | Purpose | Phase |
|------|---------|-------|
| `error.rs` | `ProtocolError` (thiserror) | 8 |
| `packet.rs` | Generic packet frame (header + payload) | 8 |
| `handshake.rs` | Server handshake + capability flags | 8 |
| `auth.rs` | Authentication response handling | 8 |
| `command.rs` | COM_QUERY and friends | 8 |
| `response.rs` | OK / Error / ResultSet / EOF packets | 8 |
| `charset.rs` | Charset constants (utf8mb4 only in v1) | 8 |
