# Error Handling — `ferrumdb-protocol`

## Planned Error Type

```rust
// crates/ferrumdb-protocol/src/error.rs (planned)
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// Packet length field exceeds 0xFF_FF_FF.
    #[error("packet too large: {0}")]
    PacketTooLarge(usize),

    /// Truncated payload (read returned fewer bytes than the header claimed).
    #[error("truncated payload at seq {0}")]
    Truncated(u8),

    /// Unknown command byte in COM_* dispatch.
    #[error("unknown command: {0}")]
    UnknownCommand(u8),

    /// Client and server could not agree on capabilities.
    #[error("capability mismatch: missing {0:#x}")]
    MissingCapability(u32),

    /// Handshake failed before authentication could complete.
    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    /// Wrapped I/O error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

## Conventions

- `thiserror::Error` derive
- `#[from] std::io::Error` for transport errors
- Specific variants; **no** `Internal(String)`
- Errors map to the MySQL `Error` packet with a stable `error_code`

## Mapping to MySQL error codes

| Variant | MySQL error code |
|---------|------------------|
| `PacketTooLarge` | 1158 (ER_NET_PACKET_TOO_LARGE) |
| `Truncated` | 1159 (ER_NET_READ_INTERRUPTED) |
| `UnknownCommand` | 1047 (ER_UNKNOWN_COM_ERROR) |
| `MissingCapability` | 1156 (ER_NOT_SUPPORTED_YET) |
| `HandshakeFailed` | 1045 (ER_ACCESS_DENIED_ERROR) |

## Critical Safety Rules

- **Never** silently truncate a packet that exceeds `0xFF_FF_FF` — return `PacketTooLarge`
- **Never** invent a fake capability bit to make handshake "succeed"

## Anti-Patterns

- ❌ Using `String` for an error that has a fixed taxonomy (add a variant)
- ❌ Logging inside packet decode — let the caller log with connection context
- ❌ `panic!` on protocol violations — return the error so the server can drop the connection gracefully
