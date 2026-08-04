# Backend Development Guidelines — `ferrumdb-btree`

> B+Tree index for FerrumDB: in-memory (phase 2), persistent (phase 3), and secondary indexes with primary-key lookup (phase 6).

---

## Overview

`ferrumdb-btree` owns the on-disk and in-memory B+Tree implementation. Today
the crate is a stub (`crates/ferrumdb-btree/src/lib.rs`) with module-level
documentation and a `crate_compiles` smoke test only. All real implementation
lands in phase 2 onwards per `docs/plan.md`.

The crate will eventually expose:
- `BTree<P, K, V>` — generic over the page source (memory or `ferrumdb-page`)
- `Node` / `Internal` / `Leaf` — node kinds
- `PageId` newtype for node persistence
- `insert`, `get`, `delete`, `scan` operations
- Range iterator with bidirectional leaf chain for `scan_range`

`ferrumdb-btree` depends only on `ferrumdb-page` and `thiserror`. It must
NOT depend on `ferrumdb-buffer`, `ferrumdb-wal`, or `ferrumdb-space` — those
are wired in by `ferrumdb-engine` (phase 7).

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Current stub + planned files | Filled |
| [Database Guidelines](./database-guidelines.md) | B+Tree layout: leaf chain, internal node routing, split rules | Filled |
| [Error Handling](./error-handling.md) | Planned `BTreeError` variants and propagation | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Split-threshold constants, generic constraints, tests | Filled |

---

## Pre-Development Checklist

Before writing any code in this crate:

- [ ] Decide whether the implementation is in-memory (phase 2) or persistent (phase 3) — they share node types but differ in page I/O
- [ ] If persistent, decide how `Node` serialises into `ferrumdb_page::Page` user data — get the on-disk layout reviewed before coding
- [ ] Use a `const` for split threshold (e.g., `ORDER` and `MIN_KEYS`); never inline magic numbers
- [ ] Generic bounds: `K: Ord + Clone`, `V: Clone` — confirm before adding new ones
- [ ] Iterator returns `impl Iterator<Item = Result<(K, V), BTreeError>>` not `Vec` to avoid materialising large scans
- [ ] If touching the leaf chain, confirm bidirectional links (`next_leaf`, `prev_leaf`) are updated on every split / merge

---

## Quality Check (Reviewer Gate)

- [ ] Module-level `//!` doc updated with new phase reference if scope changed
- [ ] All `pub` items have `///` doc comments (`#![deny(missing_docs)]` is set in `crates/ferrumdb-btree/src/lib.rs:18`)
- [ ] Split / merge code paths have unit tests with random keys (≥ 1 000 insertions)
- [ ] Range scan correctness tested at the boundaries (empty range, single-key range, full-range)
- [ ] No direct dependency on `ferrumdb-buffer` / `ferrumdb-wal` / `ferrumdb-space` — those belong to `ferrumdb-engine`
- [ ] Error type lives in `error.rs` and uses `thiserror`
