# Directory Structure — `ferrumdb-wal`

Real layout (2026-07-18, post phase-5 implementation):

```
crates/ferrumdb-wal/
├── Cargo.toml              # ferrumdb-page + thiserror + crc32fast; dev: tempfile + ferrumdb-space
└── src/
    ├── lib.rs              # Module doc + #![deny(missing_docs)] + re-exports + 15 unit tests (including M1)
    ├── error.rs            # WalError enum (thiserror) — phase 5
    └── record.rs           # RedoRecord struct + encode/decode + 5 unit tests — phase 5
    # Note: wal.rs was originally planned; folded into lib.rs in v1 for simplicity.
```

## File responsibilities

| File | Purpose | Public surface |
|------|---------|----------------|
| `lib.rs` | Crate-level `//!` doc + `#![deny(missing_docs)]` + `pub use` re-exports + `Wal` struct (open/create/append/fsync/checkpoint/recover) + 15 unit tests | `Wal`, `RedoRecord`, `WalError`, `CHECKPOINT_MAGIC`, `HEADER_BYTES`, `CHECKPOINT_SLOT_BYTES` |
| `error.rs` | `WalError` enum | `Io`, `RecordCrcMismatch { lsn }`, `Truncated`, `InvalidRecord(String)`, `CheckpointCorrupt`, `LsnExhausted`, `OutOfOrder { expected, got }` |
| `record.rs` | `RedoRecord` struct + encode/decode | `RedoRecord { lsn, page_id, offset, payload }` + `encode()` + `decode(bytes, expected_lsn)` + `encoded_len()` + `CHECKPOINT_MAGIC` |

## Visibility conventions

- All inter-module declarations are private
- Everything public is re-exported from `lib.rs`
- `Wal.file` and `Wal.path` are private (caller uses API methods only)
- `Wal::open` is `pub`; `read_all` / `decode_checkpoint_lsn` / `is_checkpoint_record` are file-private helpers
