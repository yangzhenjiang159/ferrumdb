# Backend Development Guidelines — `ferrumdb-txn`

> Transactions, undo log, read view, and MVCC for FerrumDB.

---

## Overview

`ferrumdb-txn` owns the transaction lifecycle (`BEGIN` / `COMMIT` / `ROLLBACK`),
the undo log (old-version storage for rollback and MVCC), and the `ReadView`
(snapshot isolation state). It also implements the MVCC version-chain walk
that lets readers see consistent snapshots without blocking writers.

Today the crate is a stub (`crates/ferrumdb-txn/src/lib.rs`). Real
implementation lands in phase 9 (transactions) and phase 10 (MVCC) per
`docs/plan.md`.

The crate depends on `ferrumdb-page` (for `Value`, `Row`, page-level lsn) and
`ferrumdb-wal` (for the redo side of commit). It does **not** depend on
`ferrumdb-btree`, `ferrumdb-buffer`, or `ferrumdb-engine` — those wire txn in
later.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Stub + planned files | Filled |
| [Database Guidelines](./database-guidelines.md) | Txn ID, undo log, ReadView, MVCC walk | Filled |
| [Error Handling](./error-handling.md) | Planned `TxnError` | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Idempotency, isolation level, tests | Filled |

---

## Pre-Development Checklist

- [ ] Decide isolation level for first version — `docs/plan.md` recommends starting with **Read Committed** or a simplified **Repeatable Read**
- [ ] `TransactionId` is `u64`, globally monotonic (see `ferrumdb-engine::TransactionId` type alias in `crates/ferrumdb-engine/src/engine.rs:6`)
- [ ] Undo log records old row versions before any `update` / `delete`
- [ ] Commit barrier: `wal.append(commit_record)` + `wal.fsync()` before marking the txn committed
- [ ] `ReadView` captures the snapshot at first read in the txn
- [ ] Row version chain traversal: row header gains `trx_id` and `roll_ptr` (phase 10)
- [ ] No `async` — transactions are sync

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references phase 9 (txn) and phase 10 (MVCC)
- [ ] `TxnError` in `error.rs`, `thiserror` derive
- [ ] All `pub` items have `///` doc comments
- [ ] Rollback test: insert + rollback → row invisible after restart
- [ ] Commit test: insert + commit → row visible to a fresh txn
- [ ] Snapshot read test: writer's uncommitted changes invisible to a snapshot started before commit
- [ ] Write-skew test documented (even if "not yet implemented")
