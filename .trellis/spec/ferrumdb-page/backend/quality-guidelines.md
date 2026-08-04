# Quality Guidelines — `ferrumdb-page`

## Required Patterns

1. **`#![deny(missing_docs)]`** at the top of `lib.rs` — already set in
   `crates/ferrumdb-page/src/lib.rs:7`. Every new `pub` item must have a
   `///` doc comment or the crate fails to compile.

2. **Module-level `//!` doc** in `lib.rs` describing responsibilities in
   Chinese + `docs/plan.md` stage reference. Pattern:
   ```rust
   //! <一句话职责>。
   //!
   //! # 职责
   //! - ...
   //!
   //! 见项目文档 `docs/plan.md` 阶段 <N>。
   ```

3. **Error type lives in its own file** (`error.rs`), not inlined into `lib.rs`.

4. **`thiserror::Error` derive** for every error enum — see
   `crates/ferrumdb-page/src/error.rs`.

5. **`#[repr(u8)]` + `to_u8()` / `from_u8() -> Result<_, _>`** for every
   enum that crosses the disk boundary. Reference: `PageType` in
   `crates/ferrumdb-page/src/page.rs:60-91`.

6. **Real-path doc links** in doc comments: `` [`Page`] ``, `` [`PAGE_SIZE`] ``
   — uses intra-doc links, not raw paths.

7. **Unit tests** at the bottom of each module under `#[cfg(test)] mod tests`.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| `unwrap()` / `expect()` in non-test code | Library code must propagate via `Result` |
| `panic!` in business logic | Same — return `PageError` instead |
| Hand-rolled `impl std::error::Error` for new error types | Use `#[derive(thiserror::Error)]` |
| Logging via `tracing` / `log` | Pure library — see `logging-guidelines.md` |
| Redefining `PAGE_SIZE` in another crate | Single source of truth at `crates/ferrumdb-page/src/page.rs:14` |
| Big-endian serialisation | Project convention is little-endian — `crates/ferrumdb-page/src/page.rs:6` |
| Silently coercing bytes via `as u8` | Use the `to_u8` / `from_u8` pattern |
| Renumbering existing `PageType` variants | Breaks every existing tablespace on disk |
| Adding fields without bumping `PAGE_HEADER_VERSION` | Reader can't distinguish old vs new layout |

## Testing Requirements

Every change touching disk format **must** add a test that:

1. Round-trips the changed shape via `Page::to_bytes` / `Page::from_bytes`
2. Flips at least one byte and asserts the expected `PageError` variant
3. Uses `assert_eq!` (not `assert!`) so the failure message shows actual vs expected

Reference test names in `crates/ferrumdb-page/src/page.rs`:
- `page_round_trip`
- `page_checksum_detects_corruption`
- `page_invalid_length`
- `page_invalid_magic`
- `page_type_round_trip`
- `page_new_initializes_header`

## Code Review Checklist

Before approving a PR that touches `ferrumdb-page`:

- [ ] Disk layout diagram in module `//!` doc is still accurate
- [ ] All four `PageError` variants are still reachable from a unit test
- [ ] `cargo test -p ferrumdb-page` passes with no warnings
- [ ] No new external dependencies added to `Cargo.toml` without updating `docs/dependencies.md`
- [ ] If `PAGE_HEADER_VERSION` was bumped, migration story is documented (how to detect old-format pages)
- [ ] If a new `PageType` was added, the value is the next free `u8` and is listed in the `PageType::from_u8` match
