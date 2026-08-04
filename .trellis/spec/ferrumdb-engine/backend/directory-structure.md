# Directory Structure — `ferrumdb-engine`

Real layout (2026-07-18):

```
crates/ferrumdb-engine/
├── Cargo.toml              # depends on all storage crates: page + btree + buffer + wal + space + txn
└── src/
    ├── lib.rs              # Crate-level module doc + re-exports
    └── engine.rs           # StorageEngine trait + EngineError + RangeBound + RowIterator + TransactionId
```

`crates/ferrumdb-engine/src/engine.rs` is the only meaningful source file today.
It defines all the public types listed in `index.md` above.

## Planned (not yet present)

| File | Purpose | Phase |
|------|---------|-------|
| `ferrum.rs` | `FerrumEngine` struct implementing `StorageEngine` | 7 |
| `catalog.rs` | `TableCatalog` (table name → schema + root page_ids) | 7 |
| `ddl.rs` | DDL plumbing (`create_table`, `drop_table`) | 7 |
| `dml.rs` | DML plumbing (`insert`, `update`, `delete`, `get_by_pk`) | 7 |
| `scan.rs` | Range-scan implementation (`scan` returns `RowIterator`) | 7 |
| `txn_bridge.rs` | Integration with `ferrumdb-txn` (phase 9+ `begin` / `commit` / `rollback`) | 9 |
| `error.rs` | `EngineError` moved out of `engine.rs` into its own file | 7 (cleanup) |

## Conventions

- The trait stays in `engine.rs` even after implementation lands — it is the public contract
- The implementation files (`ferrum.rs`, `catalog.rs`, etc.) are private modules re-exported only via a single `pub use ferrum::FerrumEngine` if needed
- `error.rs` split is optional but recommended for readability once `EngineError` grows
