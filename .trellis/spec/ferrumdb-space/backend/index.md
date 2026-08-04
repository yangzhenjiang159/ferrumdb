# Backend Development Guidelines — `ferrumdb-space`

> Tablespace: the on-disk file that holds every FerrumDB page. Manages the superblock, page-id → file-offset mapping, and free-page allocation.

---

## Overview

`ferrumdb-space` owns the `tablespace.ibd` file (or any path passed to
`Space::open`). It maps `PageId` to byte offset (`page_id * PAGE_SIZE`),
manages the superblock at page 0, and tracks the free-page list for
allocation.

Today the crate is a stub (`crates/ferrumdb-space/src/lib.rs`). Real
implementation lands in phase 3 per `docs/plan.md`.

The crate depends on `ferrumdb-page` (for the `Page` type and `PAGE_SIZE`)
and `thiserror`. It does **not** depend on `ferrumdb-buffer` — the buffer
pool calls into `Space`, never the other way around.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Stub + planned files | Filled |
| [Database Guidelines](./database-guidelines.md) | File format, superblock, page allocation | Filled |
| [Error Handling](./error-handling.md) | Planned `SpaceError` | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging | Filled |
| [Quality Guidelines](./quality-guidelines.md) | fsync, file-offset invariants | Filled |

---

## Pre-Development Checklist

- [ ] Single source of truth for `PAGE_SIZE`: re-export from `ferrumdb_page`, never redefine
- [ ] Superblock is page 0 with `PageType::Superblock`
- [ ] File offset for page `n` is exactly `n * PAGE_SIZE`
- [ ] Free-page list is a linked list through `PageType::Free` pages
- [ ] Extending the file uses `set_len` then writes the new free page
- [ ] `OpenOptions::create_new` for new tablespaces, `OpenOptions::open` for existing
- [ ] Superblock read on every `Space::open` and validated; refuses to open if magic wrong

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references `docs/plan.md` phase 3
- [ ] `SpaceError` in `error.rs`, `thiserror` derive, with `#[from] std::io::Error`
- [ ] All `pub` items have `///` doc comments
- [ ] Superblock read test: corrupt page 0 magic, expect `SuperblockInvalidMagic`
- [ ] Page allocation test: allocate all free pages, then expect `NoFreePage`
- [ ] No `PAGE_SIZE` constant in this crate's source
