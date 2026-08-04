# Quality Guidelines — `ferrumdb-space`

## Required Patterns

1. **`#![deny(missing_docs)]`** — set in `crates/ferrumdb-space/src/lib.rs`.
2. **Module-level `//!` doc** in standard format (Chinese responsibilities + phase 3 reference).
3. **Error type in `error.rs`**, `thiserror` derive, `#[from] std::io::Error`.
4. **Single source of truth for `PAGE_SIZE`**: import from `ferrumdb_page::PAGE_SIZE`, never redefine.
5. **Page-id ↔ offset** computed in exactly one helper function (`offset_of`).
6. **`sync_all` after every metadata mutation** before returning Ok.
7. **Superblock read+validate on every `Space::open`**; refuses to open on invalid magic.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| Redefining `PAGE_SIZE` | Source-of-truth violation |
| Skipping `sync_all` after metadata write | Crash leaves the file inconsistent |
| Auto-truncating on corruption | Silent data loss |
| `panic!` on recoverable errors | Return `SpaceError` |
| Holding a `Space` borrow across an await point | This crate is sync |
| Writing to a page_id >= page_count without bumping page_count | Triggers PageIdOutOfRange |
| `unsafe` | Not needed |

## Testing Requirements

When implementation lands (phase 3 baseline):

- [x] Open existing tablespace: read superblock, validate magic
- [x] Open new tablespace: write fresh superblock
- [x] Open with bad magic: returns `SuperblockInvalidMagic`
- [ ] Open with wrong page size: returns `SuperblockPageSizeMismatch` (deferred — superblock validation doesn't yet check this in open)
- [x] Allocate pages until free list is empty; then grow the file
- [x] Free a page, re-allocate, assert same PageId returned
- [x] Read/write round-trip preserves bytes exactly
- [ ] Crash-safety: truncate file mid-write, reopen, verify superblock read still succeeds (deferred to phase 5 WAL)

## Code Review Checklist

- [ ] Module `//!` doc references phase 3
- [ ] No new external dep beyond `ferrumdb-page`, `thiserror`
- [ ] No `PAGE_SIZE` constant defined in this crate
- [ ] `sync_all` called on every metadata mutation
- [ ] All `pub` items have `///` doc comments
- [ ] `cargo test -p ferrumdb-space` passes
- [ ] `cargo clippy -p ferrumdb-space -- -D warnings` clean
