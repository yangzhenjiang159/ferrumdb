# Backend Development Guidelines — `ferrumdb-page`

> 16KB fixed-size page format: header, footer, slotted user data, CRC32 checksum, and on-disk row encoding for FerrumDB.

---

## Overview

`ferrumdb-page` is the lowest layer of FerrumDB's storage stack. It defines the
on-disk page format used by every other crate (`btree`, `wal`, `space`,
`buffer`, `engine`, `txn`, `sql`, `server`). It owns the wire/in-memory types
`Page`, `PageHeader`, `PageFooter`, `PageType`, `Row`, `Value`, `Schema`, and
the error type `PageError`.

The crate is in **phase 1** per `docs/plan.md`. Today it ships a complete
header/footer + CRC32 implementation plus stubs for row encoding. Future phases
will add slotted-page layout, NULL bitmap, and variable-length encoding.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organisation: `lib.rs`, `page.rs`, `row.rs`, `error.rs` | Filled |
| [Database Guidelines](./database-guidelines.md) | On-disk page layout, magic, byte order, `repr(u8)` enums | Filled |
| [Error Handling](./error-handling.md) | `PageError` variants, propagation, `thiserror` usage | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library crate — no logs; cross-ref `ferrumdb-server` | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Forbidden patterns, required patterns, testing rules | Filled |

---

## Pre-Development Checklist

Before changing any type in `ferrumdb-page`, confirm:

- [ ] Any new page-header field has a reserved offset range that does not shift existing fields (`crates/ferrumdb-page/src/page.rs:31-44`)
- [ ] Any new `PageType` variant is appended at the next free `repr(u8)` value and round-trip-tested through `PageType::from_u8` (`crates/ferrumdb-page/src/page.rs:60-91`)
- [ ] Both header.checksum AND footer.checksum are updated together (`crates/ferrumdb-page/src/page.rs`)
- [ ] New public item has a `///` doc comment (denied by `#![deny(missing_docs)]` in `crates/ferrumdb-page/src/lib.rs:7`)
- [ ] `Row` / `Value` / `Schema` change is reflected in `ferrumdb-engine`'s trait signatures (`crates/ferrumdb-engine/src/engine.rs:33`)
- [ ] A corruption-detection test is added if a checksum, magic, or length invariant is touched
- [ ] Module-level `//!` doc still references the right `docs/plan.md` stage number

---

## Quality Check (Reviewer Gate)

A change to `ferrumdb-page` is ready when:

- [ ] Error variants use `#[derive(Debug, thiserror::Error)]` with `#[error("...")]` (see `crates/ferrumdb-page/src/error.rs`)
- [ ] All public items have doc comments (enforced by lint)
- [ ] Unit tests cover happy-path round-trip + at least one corruption / invalid-input case (`crates/ferrumdb-page/src/page.rs` tests at end of file)
- [ ] Byte-order convention (little-endian) is documented wherever it could be ambiguous
- [ ] Disk-format invariants are documented in the module-level `//!` doc with an ASCII layout diagram
- [ ] The 16 384-byte page size is never silently re-defined elsewhere — only `PAGE_SIZE` constant in `crates/ferrumdb-page/src/page.rs:14` is the source of truth
- [ ] No `unwrap()` / `expect()` in non-test code paths
