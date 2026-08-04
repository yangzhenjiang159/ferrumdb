# Directory Structure — `ferrumdb-txn`

Real layout (2026-07-18):

```
crates/ferrumdb-txn/
├── Cargo.toml              # ferrumdb-page + ferrumdb-wal + thiserror
└── src/
    └── lib.rs              # Module doc + #![deny(missing_docs)] + crate_compiles smoke test
```

`crates/ferrumdb-txn/src/lib.rs:14-16` defines the placeholder test.

## Planned (not yet present)

| File | Purpose | Phase |
|------|---------|-------|
| `error.rs` | `TxnError` (thiserror) | 9 |
| `id.rs` | `TransactionId` allocation + active-txn table | 9 |
| `txn.rs` | `Transaction` struct (state machine) | 9 |
| `undo.rs` | `UndoRecord` + undo log | 9 |
| `read_view.rs` | `ReadView` snapshot | 10 |
| `mvcc.rs` | Version-chain walk for snapshot reads | 10 |
| `isolation.rs` | Isolation-level enum + visibility rules | 10 |
