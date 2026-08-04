# Quality Guidelines — `ferrumdb-buffer`

## Required Patterns

1. **`#![deny(missing_docs)]`** — set in `crates/ferrumdb-buffer/src/lib.rs`.
2. **Module-level `//!` doc** in standard format (Chinese responsibilities + phase 4 reference + lock-ordering note).
3. **Error type in `error.rs`**, `thiserror` derive.
4. **`Box<dyn PageSource>` for source abstraction** — enables mock injection in tests.
5. **`PageGuard` RAII**: `Deref<Target=Page>` + `DerefMut` + `Drop` for automatic unpin.
6. **Capacity >= 1** — asserted in `with_source`.
7. **Dirty page flushed before eviction** — enforced in `evict_lru` algorithm.
8. **No `unsafe`** — sync library, no perf hacks needed.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| `unwrap()` / `expect()` in non-test code | Hot path; return `BufferError` instead |
| Holding `PageGuard` and calling `pool.fetch_page` simultaneously | Borrow checker conflict; drop first |
| Modifying `BufferPool.table` directly (bypassing `fetch_page`) | Invariant violation |
| Holding `&mut BufferPool` across `await` point | v1 is sync; v2 multi-thread needs lock discipline |
| Skipping `flush_all` on shutdown | Data loss |
| Evicting dirty frame without flushing | Data loss |
| `clone()` of `Page` in `BufferPoolSource::read_page` for performance | Acceptable in v1 (only happens on miss); v2 can use `Arc<Page>` |
| Returning `Ok(())` from `flush_all` while skipping dirty frames | Silent data loss |

## Lock Ordering (v2 multi-thread)

When implementing multi-thread support, the order is:

1. Pool lock (covers `frames`, `table`, `lru_order`)
2. Frame-level lock (per-frame, if added in v2)

**Reverse order → deadlock.** Document in module doc.

## Testing Requirements (phase 4 baseline)

| Method | Minimum tests |
|--------|---------------|
| `fetch_page` | basic + cache hit (re-fetch same id) + LRU eviction |
| `allocate_page` | (covered indirectly by `PersistentBtree` integration test) |
| `flush_all` | multiple dirty frames → flush_all → 0 dirty |
| `PageGuard::drop` | unpin path (test via successful re-fetch) |
| `page_mut` | mark dirty (test via flush_all count) |
| Eviction | LRU + dirty-before-evict (covered by dirty_page_flushed_before_eviction test) |
| Pin protection | pinned_page_not_evicted (LRU respects access order) |
| BufferPoolSource + PersistentBtree | basic insert + get (in `ferrumdb-btree::persistent::buffer_pool_integration`) |

## Known Deferred Tests (v2)

- "PoolFull while pinned" — requires interior mutability; deferred to multi-threaded v2
- `FrameNotFound` internal state — only triggered by misuse, no current test path
- Mock source returning `Io` / `Page` / `Space` errors — would require more elaborate MockSource; current MockSource is too simple

## Code Review Checklist

- [ ] No new external dep beyond `ferrumdb-page`, `ferrumdb-space`, `thiserror` (+ tempfile dev)
- [ ] All `pub` items have `///` doc comments
- [ ] PageGuard drop is the only path that decrements pin_count
- [ ] `evict_lru` flushes dirty before evicting
- [ ] `cargo test -p ferrumdb-buffer` passes
- [ ] `cargo clippy -p ferrumdb-buffer -- -D warnings` clean
