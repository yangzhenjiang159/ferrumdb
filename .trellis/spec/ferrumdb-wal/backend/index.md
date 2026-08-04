# Backend Development Guidelines — `ferrumdb-wal`

> Redo Log (WAL): append-only redo records, fsync, checkpoint, crash recovery.

---

## Overview

`ferrumdb-wal` owns the redo log that makes FerrumDB crash-safe. It writes
physical redo records `(lsn, page_id, offset, payload)` to an append-only file
**before** any page mutation, and replays them at startup to recover committed
changes.

Phase 5 implements the core WAL:
- `Wal::create` / `open` / `append` / `fsync` / `checkpoint` / `recover`
- `RedoRecord` codec with CRC32 integrity check
- 14 unit tests + 1 M1 end-to-end integration test (WAL + Space crash recovery)

Real implementation in `crates/ferrumdb-wal/src/`.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | 3 source files | Filled |
| [Database Guidelines](./database-guidelines.md) | File format + record encoding + recovery algorithm | Filled |
| [Error Handling](./error-handling.md) | `WalError` 7 variants | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging | Filled |
| [Quality Guidelines](./quality-guidelines.md) | fsync rules, CRC, Truncated handling, M1 tests | Filled |

---

## Pre-Development Checklist

- [ ] Don't redefine record format; extend via new variants
- [ ] Always fsync after append AND after header rewrite
- [ ] Checkpoint at fixed slot (8-19), not append-at-end
- [ ] `Truncated` is NOT an error; only `RecordCrcMismatch` is fatal
- [ ] Header `next_lsn` may be ahead of actual records (in-progress append); use max()
- [ ] `recover` propagates `RecordCrcMismatch` to caller (no silent skip)

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references phase 5
- [ ] `WalError` uses `thiserror`
- [ ] All `pub` items have `///` doc comments
- [ ] CRC32 covers `lsn || page_id || offset || payload_len || payload`
- [ ] Checkpoint slot is fixed bytes 8-19 (header 0-7, records from 20)
- [ ] `m1_crash_recovery_replays_records` test passes (simulates kill before checkpoint)
- [ ] `m1_wal_plus_space_crash_recovery` test passes (WAL + Space integration)
- [ ] `cargo test -p ferrumdb-wal` passes (15 tests)
- [ ] `cargo clippy -p ferrumdb-wal -- -D warnings` clean
