# Backend Development Guidelines — `ferrumdb-engine`

> StorageEngine facade: the single public API that SQL layer and integration tests use to talk to FerrumDB.

---

## Overview

`ferrumdb-engine` owns the `StorageEngine` trait and the default
implementation `FerrumEngine`. It is the integration point that wires
together `ferrumdb-page`, `ferrumdb-btree`, `ferrumdb-buffer`,
`ferrumdb-wal`, `ferrumdb-space`, and `ferrumdb-txn`.

The trait skeleton lives in `crates/ferrumdb-engine/src/engine.rs` —
`StorageEngine`, `EngineError`, `RangeBound`, `RowIterator`, `IndexMeta`,
and `TransactionId`. Since **阶段 6** there is a minimal implementation
`FerrumEngine` (`crates/ferrumdb-engine/src/ferrum_engine.rs`) with an
in-memory catalog (`catalog.rs`): it implements `create_table` /
`create_index` / `insert` / `get_by_pk` / `get_by_index` / `scan` /
`scan_index` on top of `ferrumdb-space::Space` + `PersistentBtree`; other
methods return `Unsupported` until phase 7/9.

Real definitions from `crates/ferrumdb-engine/src/engine.rs`:

```rust
pub type TransactionId = u64;                                // line 6
pub struct RangeBound { pub start: Option<Value>, pub end: Option<Value> }  // line 17
pub type RowIterator<'a> = Box<dyn Iterator<Item = Result<Row, EngineError>> + 'a>;  // line 30
pub enum EngineError { TableNotFound, DuplicateKey, Unsupported, Internal } // line 33
pub struct IndexMeta { name, columns: Vec<usize>, is_unique } // 阶段 6
pub trait StorageEngine { /* create_table, drop_table, insert, update, delete,
                              get_by_pk, scan, create_index, get_by_index,
                              scan_index, begin, commit, rollback */ }
```

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Real files + planned implementation files | Filled |
| [Database Guidelines](./database-guidelines.md) | Trait contracts, integration wiring | Filled |
| [Error Handling](./error-handling.md) | `EngineError` real + planned extensions | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging (caller logs) | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Trait design rules, doc comments, tests | Filled |

---

## Pre-Development Checklist

Before changing the `StorageEngine` trait or its implementors:

- [ ] Trait methods have doc comments listing the planned phase for each (see `crates/ferrumdb-engine/src/engine.rs` "实现阶段" table)
- [ ] `# Errors` section in each method's doc comment enumerates `EngineError` variants it can return
- [ ] `EngineError` variants cover: user-visible (`TableNotFound`, `DuplicateKey`), not-yet-implemented (`Unsupported`), system-level (`Internal`)
- [ ] Method additions keep `dyn StorageEngine` object-safe (no generic methods, no `Self` in return types beyond `Self: Sized` defaults)
- [ ] No business logic in `engine.rs`; that belongs in `ferrumdb.rs` (phase 7)
- [ ] Update `docs/plan.md` "StorageEngine trait 方法清单" table when a method's phase changes

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references `docs/plan.md` phase 7
- [ ] All `pub` items have `///` doc comments
- [ ] `EngineError` uses `thiserror` (verified in `crates/ferrumdb-engine/src/engine.rs:33`)
- [ ] Every trait method has `# Errors` documentation
- [ ] `engine_is_object_safe` test in `crates/ferrumdb-sql/src/lib.rs:14-16` continues to pass after any change
