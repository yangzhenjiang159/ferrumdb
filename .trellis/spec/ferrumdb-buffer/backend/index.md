# Backend Development Guidelines — `ferrumdb-buffer`

> Buffer Pool: in-memory page cache with pin/unpin semantics, LRU eviction, and dirty page flush.

---

## Overview

`ferrumdb-buffer` caches `ferrumdb_page::Page` instances in memory, manages
pin counts via RAII guards (`PageGuard`), evicts cold pages with an LRU
policy, and flushes dirty pages back to disk via `ferrumdb-space`.

Phase 4 implements the core Buffer Pool:
- `BufferPool` (Vec<Frame> + HashMap<PageId, FrameId> + LruOrder)
- `PageGuard<'a>` RAII pin holder
- `BufferPoolSource<'a>` adapter exposing `BufferPool` as `PageSource`
- LRU + dirty-before-evict
- 8 unit tests + 1 integration test (PersistentBtree through BufferPool)

Real implementation in `crates/ferrumdb-buffer/src/`.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | 6 source files + 1 binary | Filled |
| [Database Guidelines](./database-guidelines.md) | Frame table, LRU, dirty flush, API summary | Filled |
| [Error Handling](./error-handling.md) | `BufferError` 6 variants | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging | Filled |
| [Quality Guidelines](./quality-guidelines.md) | PageGuard RAII, lock ordering, tests | Filled |

---

## Pre-Development Checklist

Before changing `BufferPool` or `PageGuard`:

- [ ] `PageGuard` is the only path that decrements `pin_count` (no public `unpin`)
- [ ] Dirty frames are flushed before eviction (`evict_lru` algorithm)
- [ ] Capacity is `>= 1` (asserted in `with_source`)
- [ ] `Box<dyn PageSource>` is the source abstraction (allows mock injection)
- [ ] Lock-ordering note in `lib.rs` is intact (v1 single-threaded but v2 needs it)
- [ ] `BufferPoolSource` impl `PageSource` returns `SpaceError` (not `BufferError`) — caller has only `PageSource` trait

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references phase 4 + lock-ordering note
- [ ] `BufferError` uses `thiserror` (verified in `crates/ferrumdb-buffer/src/error.rs`)
- [ ] All `pub` items have `///` doc comments
- [ ] PageGuard drop is the only path that decrements pin_count
- [ ] `evict_lru` flushes dirty before evicting
- [ ] `cargo test -p ferrumdb-buffer` passes (8 unit + 1 integration)
- [ ] `cargo clippy -p ferrumdb-buffer -- -D warnings` clean
- [ ] No new external dep beyond `ferrumdb-page`, `ferrumdb-space`, `thiserror`
