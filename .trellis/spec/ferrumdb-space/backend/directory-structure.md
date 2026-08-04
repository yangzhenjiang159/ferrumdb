# Directory Structure — `ferrumdb-space`

Real layout (2026-07-18, post phase-3 implementation):

```
crates/ferrumdb-space/
├── Cargo.toml              # ferrumdb-page + thiserror; dev: tempfile
└── src/
    ├── lib.rs              # Module doc + #![deny(missing_docs)] + re-exports + crate_compiles smoke test
    ├── error.rs            # SpaceError enum (thiserror) — phase 3
    ├── superblock.rs       # Superblock struct + write_into/read_from + 3 unit tests — phase 3
    ├── free_list.rs        # encode_free / decode_free helpers + 3 unit tests — phase 3 (pub(crate))
    ├── page_source.rs      # PageSource trait + impl for &mut T / Box<T> / Space — phase 3
    └── space.rs            # Space struct + open/create/read/write/alloc/free + 7 unit tests — phase 3
```

## File responsibilities

| File | Purpose | Public surface |
|------|---------|----------------|
| `lib.rs` | Crate-level `//!` doc + `#![deny(missing_docs)]` + `pub use` re-exports | `Space`, `Superblock`, `SpaceError`, `PageSource` |
| `error.rs` | `SpaceError` enum | `Io`, `PageIdOutOfRange`, `FreeListCorrupted`, `SuperblockInvalidMagic`, `SuperblockPageSizeMismatch { file, build }`, `SuperblockVersionUnsupported`, `NotInitialized`, `SuperblockTruncated { got }` |
| `superblock.rs` | `Superblock` struct + 26-byte binary codec | `Superblock::fresh`, `write_into`, `read_from` |
| `free_list.rs` | Free-page user_data codec (5 bytes) | `encode_free(Option<u32>)`, `decode_free(&[u8])` — pub(crate) |
| `page_source.rs` | `PageSource` trait abstraction | `trait PageSource { read_page, write_page, allocate_page }` + blanket impls |
| `space.rs` | `Space` main struct | `Space::create`, `open`, `close` (via Drop), `read_page`, `write_page`, `allocate_page`, `free_page`, `set_root_page_id`, `sync_all`, `path`, `page_count`, `superblock` |

## Visibility conventions

- All inter-module declarations are private: `mod error;`, `mod superblock;`, `mod free_list;`, `mod page_source;`, `mod space;`
- Everything public is re-exported from `lib.rs`
- `Space` consumes itself in `close()` (currently relies on `Drop` for file handle)
- `free_list` is pub(crate) — only used inside `space.rs`

## Phase-3 Implementation Notes

- **`PAGE_SIZE` is imported from `ferrumdb-page`** — never redefined
- **`PAGE_MAGIC` re-imported only in tests** via `#[cfg(test)]` (avoids runtime unused warning)
- **`offset_of(page_id) = page_id * PAGE_SIZE`** — single helper in `space.rs`
- **`sync_all()`** called after every write_page / allocate_page / free_page / set_root_page_id / write_superblock
- **Free list**: singly-linked list of `PageType::Free` pages; each free page's user_data carries `[is_some:u8][next_id:u32 LE]`
- **File extension**: `set_len((page_count+1) * PAGE_SIZE)`; page_count incremented BEFORE write_page to avoid PageIdOutOfRange
