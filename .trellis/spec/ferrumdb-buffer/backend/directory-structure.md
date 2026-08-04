# Directory Structure — `ferrumdb-buffer`

Real layout (2026-07-18, post phase-4 implementation):

```
crates/ferrumdb-buffer/
├── Cargo.toml              # ferrumdb-page + ferrumdb-space + thiserror; dev: tempfile
└── src/
    ├── lib.rs              # Module doc + #![deny(missing_docs)] + re-exports + 8 unit tests
    ├── error.rs            # BufferError enum (thiserror) — phase 4
    ├── frame.rs            # Frame struct + FrameId type alias — phase 4
    ├── lru.rs              # LruOrder helper (Vec<FrameId>) — phase 4
    ├── pool.rs             # BufferPool 主结构 + fetch/allocate/flush/evict + manual Debug — phase 4
    ├── guard.rs            # PageGuard RAII — phase 4
    └── source.rs           # BufferPoolSource adapter implementing PageSource — phase 4
```

## File responsibilities

| File | Purpose | Public surface |
|------|---------|----------------|
| `lib.rs` | Crate-level `//!` doc + `#![deny(missing_docs)]` + `pub use` re-exports | `BufferPool`, `PageGuard`, `Frame`, `FrameId`, `BufferError`, `BufferPoolSource` |
| `error.rs` | `BufferError` enum | `Io`, `Page(#[from] PageError)`, `Space(#[from] SpaceError)`, `PoolFull`, `FrameNotFound(usize)`, `PageNotInPool(u32)` |
| `frame.rs` | `Frame` struct | `Frame { page_id, page, pin_count, is_dirty }` + `Frame::free()` + `is_evictable()` |
| `lru.rs` | `LruOrder` Vec-based LRU tracker | `LruOrder::new`, `touch`, `remove`, `iter_lru` |
| `pool.rs` | `BufferPool` main struct | `with_source`, `open`, `create`, `fetch_page`, `allocate_page`, `flush_all`, `set_root_page_id` (placeholder), `capacity`, `used_frames`, `dirty_frame_count` |
| `guard.rs` | `PageGuard<'a>` RAII | `id`, `page`, `page_mut`, `mark_dirty` + `Deref`/`DerefMut` + `Drop` |
| `source.rs` | `BufferPoolSource<'a>` adapter | `new`, `pool_mut` + `impl PageSource` |

## Visibility conventions

- All inter-module declarations are private
- Everything public is re-exported from `lib.rs`
- `BufferPool.table` is `pub` (not `pub(crate)`) to enable test assertions — it's still opaque to most callers; `fetch_page` is the supported API
- `pool.rs` `unpin` / `mark_dirty` / `frame_page` / `frame_page_mut` / `frame_page_id` are `pub(crate)` — only `PageGuard` calls them
- `lru.rs` `peek_lru` and `len`/`is_empty` were removed during phase 4 (dead code)

## Phase-4 Implementation Notes

- **Capacity >= 1** (asserted in `with_source`)
- **`Box<dyn PageSource>`** for source abstraction; allows mock injection in tests
- **`set_root_page_id` is a placeholder** that returns `BufferError::FrameNotFound(0)` — proper implementation requires downcasting or storing an `Option<Space>`; deferred
- **Manual `impl Debug`** for `BufferPool` because `Box<dyn PageSource>` doesn't impl `Debug`
- **LRU is a `Vec<FrameId>`** — O(n) `touch` removal; capacity typically < 10k so this is fine
- **Eviction strategy**:
  1. First pass: if no clean frame found, flush ONE dirty frame
  2. Second pass: pick the LRU evictable frame
- **Borrow-checker constraint**: `PageGuard` holds `&mut BufferPool`, so tests cannot hold a guard while calling `pool.fetch_page` again. The "PoolFull" return path is implemented but hard to test without interior mutability.
