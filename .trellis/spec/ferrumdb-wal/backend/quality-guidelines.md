# Quality Guidelines — `ferrumdb-wal`

## Required Patterns

1. **`#![deny(missing_docs)]`** — set in `crates/ferrumdb-wal/src/lib.rs`.
2. **Module `//!` doc** in standard format (Chinese responsibilities + phase 5 reference).
3. **Error type in `error.rs`**, `thiserror` derive.
4. **CRC32 on every record** — covers `lsn || page_id || offset || payload_len || payload`.
5. **fsync after every append AND after every header rewrite** — both are mandatory.
6. **Checkpoint at fixed slot (bytes 8-19)** — not appended at end.
7. **`Truncated` is not an error in `recover`** — treat as normal EOF.
8. **`RecordCrcMismatch` propagates** from `recover` — caller must decide.
9. **No `unsafe`** — sync library, no perf hacks needed.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| Skipping `fsync` after `append` | Record won't survive crash |
| Appending checkpoint at end | Breaks with mixed records-after-checkpoint |
| Swallowing `RecordCrcMismatch` | Caller can't recover |
| Trusting header `next_lsn` alone without scan | Crash between record-write and header-rewrite |
| Using `String` payload | Defeats zero-copy |
| `panic!` on `OutOfOrder` | Return the error so caller can handle |
| Catching `WalError` and returning `Ok` from `recover` | Silent failure |
| `unsafe` | Not needed |

## M1 Testing Requirements (phase 5 baseline)

**M1 集成测试** (must pass):
- `m1_crash_recovery_replays_records` — append + drop + reopen + recover → records replayed
- `m1_checkpoint_then_crash_replays_only_after_checkpoint` — records before checkpoint are skipped
- `m1_wal_plus_space_crash_recovery` — full WAL + Space integration with page modification

**单元测试** (15 total):
- 5 record encode/decode round-trip + error cases
- 4 Wal create/open/append/checkpoint semantics
- 2 recover scenarios (before/after checkpoint, truncated tail)
- 1 corrupt CRC propagation
- 1 multi-page replay
- 2 M1 integration (file-level)

## Code Review Checklist

- [ ] No new external dep beyond `ferrumdb-page`, `thiserror`, `crc32fast` (+ tempfile, ferrumdb-space dev)
- [ ] All `pub` items have `///` doc comments
- [ ] fsync called after every `append` AND after every header rewrite
- [ ] Checkpoint slot is fixed bytes 8-19
- [ ] `recover` propagates `RecordCrcMismatch` (test: `recover_corrupt_crc_returns_error`)
- [ ] `recover` does not error on `Truncated` (test: `recover_handles_truncated_tail`)
- [ ] `cargo test -p ferrumdb-wal` passes (15 tests including M1)
- [ ] `cargo clippy -p ferrumdb-wal -- -D warnings` clean

## M1 集成测试模板

```rust
#[test]
fn m1_wal_plus_space_crash_recovery() {
    use ferrumdb_space::Space;
    let dir = tempfile::tempdir().unwrap();
    let wal_path = dir.path().join("m1.wal");
    let space_path = dir.path().join("m1.ibd");
    {
        let mut space = Space::create(&space_path).unwrap();
        let page_id = space.allocate_page().unwrap();
        let mut page = Page::new(page_id, PageType::Data);
        page.user_data_mut()[..N].copy_from_slice(b"...");
        space.write_page(page_id, &page).unwrap();
        let mut wal = Wal::create(&wal_path).unwrap();
        wal.append(page_id, 0, b"...").unwrap();
        drop(wal);
        drop(space);
    }
    let mut space = Space::open(&space_path).unwrap();
    let mut wal = Wal::open(&wal_path).unwrap();
    wal.recover(|rec| {
        let mut page = space.read_page(rec.page_id).unwrap();
        page.user_data_mut()[rec.offset..].copy_from_slice(&rec.payload);
        space.write_page(rec.page_id, &page).unwrap();
        Ok(())
    }).unwrap();
    let page = space.read_page(page_id).unwrap();
    assert_eq!(&page.user_data()[..N], b"...");
}
```

This template is the M1 acceptance test.
