# Error Handling — `ferrumdb-buffer`

## Current Definition (phase 4)

`crates/ferrumdb-buffer/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    #[error("buffer io: {0}")]
    Io(#[from] std::io::Error),

    #[error("page error: {0}")]
    Page(#[from] ferrumdb_page::PageError),

    #[error("space error: {0}")]
    Space(#[from] ferrumdb_space::SpaceError),

    #[error("buffer pool full (all frames pinned or dirty)")]
    PoolFull,

    #[error("frame not found: {0}")]
    FrameNotFound(usize),

    #[error("page {0} not in pool")]
    PageNotInPool(u32),
}
```

## Conventions

- `thiserror::Error` derive
- `#[from]` on wrapped errors so `?` works
- Specific variants; **no** `Internal(String)`
- `PoolFull` 不可恢复 — 调用方需要 drop 一些 `PageGuard` 或 `flush_all`
- `FrameNotFound` / `PageNotInPool` 是内部状态错误

## Variant Taxonomy

| Category | Variants |
|----------|----------|
| I/O | `Io` |
| Wrapped page error | `Page(#[from] PageError)` |
| Wrapped space error | `Space(#[from] SpaceError)` |
| Capacity | `PoolFull` |
| Internal state | `FrameNotFound`, `PageNotInPool` |

## Propagation Pattern

```rust
fn fetch_page(&mut self, page_id: u32) -> Result<PageGuard<'_>, BufferError> {
    // ...
    let page = self.source.read_page(page_id)?;   // SpaceError → BufferError via From
    // ...
}
```

## Critical Safety Rules

- **Never** skip `flush_all` before closing the pool (data loss)
- **Never** evict a dirty frame without flushing first
- **Never** increment `pin_count` past `usize::MAX` (use `saturating_add` for unpin)

## Anti-Patterns

- ❌ Adding `Internal(String)` — use specific variants
- ❌ Logging inside the pool — propagate; caller logs with context
- ❌ Catching `SpaceError` and converting to `Internal` — use `#[from]`
- ❌ Returning `Ok(())` from `flush_all` when no flush happened (silent no-op is fine but consider logging)

## Testing Errors

| Variant | Test scenario |
|---------|---------------|
| `Io(_)` | Mock source that returns I/O error (v2 — deferred) |
| `Page(_)` | Pass corrupt bytes through mock source (v2) |
| `Space(_)` | Open a tablespace with bad magic — Space::open fails, propagates to BufferPool::open |
| `PoolFull` | Pin all frames, try to fetch new one (hard to test with borrow checker; deferred to v2 with interior mutability) |
| `FrameNotFound` | Direct unit test on internal state (deferred) |
| `PageNotInPool` | (v2) Adapter-level assertion |
