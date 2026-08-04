# Database Guidelines — `ferrumdb-protocol`

## MySQL Packet Frame

```
+---------+--------+--------+
| Length  | SeqId  | Payload|
| (3B LE) | (1B)   | (N B)  |
+---------+--------+--------+
```

- Length is the size of the payload only (3-byte little-endian)
- Maximum payload size is `0xFF_FF_FF` = 16 MB; bigger payloads require splitting
- Sequence ID increments per packet per command; resets to 0 at command boundary

## Charset

Pinned at v1 per `docs/plan.md` 阶段 8 常见坑:

> 字符集先固定 `utf8mb4` 或 `latin1`，文档说明

Recommendation: `utf8mb4` (collation `utf8mb4_general_ci`). The constant
lives in `charset.rs`:

```rust
pub const CHARSET_UTF8MB4: u8 = 33;  // MySQL collation id
```

Never negotiate dynamically in v1.

## Capabilities (server-side)

Bits set in the initial handshake:

- `CLIENT_PROTOCOL_41` (0x200)
- `CLIENT_SECURE_CONNECTION` (0x8000)
- `CLIENT_TRANSACTIONS` (0x2000)
- `CLIENT_CONNECT_WITH_DB` (0x8)

Negotiation is **not** implemented; the bitmask is sent as-is and the client
is expected to agree.

## OK Packet (success)

```
+---------+------+----------+----+
| 0x00    | rows | last_id  | status
| (1B)    | (2B) | (8B)     | (2B)
+---------+------+----------+----+
```

## Error Packet

```
+---------+----------------+----+
| 0xFF    | error_code (2B)| sql_state | message
+---------+----------------+----+
```

`error_code` follows MySQL's numbering; `sql_state` is the SQL-standard 5-char code.

## ResultSet (minimal)

- Column count (length-encoded integer)
- Per column: name + type + charset (utf8mb4 only)
- EOF marker (deprecated but still used by clients; emit anyway)
- Row data: length-encoded strings / integers

## Anti-Patterns

- ❌ Negotiating charset dynamically in v1
- ❌ Supporting prepared statements (deferred per `docs/plan.md` 阶段 8)
- ❌ Hard-coding `latin1` because the MySQL docs show it first
- ❌ Returning `String` instead of `bytes::Bytes` (defeats zero-copy)
- ❌ Holding packet state across `await` (this crate is sync)
