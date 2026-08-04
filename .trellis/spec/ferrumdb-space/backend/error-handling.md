# Error Handling — `ferrumdb-space`

## Current Definition (phase 3)

`crates/ferrumdb-space/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SpaceError {
    #[error("space io: {0}")]
    Io(#[from] std::io::Error),

    #[error("page id out of range: {0}")]
    PageIdOutOfRange(u32),

    #[error("free list corrupted at page {0}")]
    FreeListCorrupted(u32),

    #[error("superblock invalid magic")]
    SuperblockInvalidMagic,

    #[error("superblock page size mismatch: file {file}, build {build}")]
    SuperblockPageSizeMismatch { file: u32, build: u32 },

    #[error("superblock version {0} not supported")]
    SuperblockVersionUnsupported(u32),

    #[error("space not initialized")]
    NotInitialized,

    #[error("superblock truncated: {got} bytes")]
    SuperblockTruncated { got: usize },
}
```

## Conventions

- `thiserror::Error` derive
- `#[from] std::io::Error` for filesystem errors
- Specific variants; **no** `Internal(String)`
- Does not derive `PartialEq + Eq` (because of wrapped `std::io::Error`)

## Variant Taxonomy

| Category | Variants |
|----------|----------|
| I/O | `Io` |
| File format | `SuperblockInvalidMagic`, `SuperblockPageSizeMismatch`, `SuperblockVersionUnsupported`, `SuperblockTruncated` |
| Allocation | `PageIdOutOfRange`, `FreeListCorrupted` |
| Lifecycle | `NotInitialized` (reserved) |

## Propagation Pattern

```rust
pub fn read_page(&mut self, page_id: u32) -> Result<Page, SpaceError> {
    if page_id >= self.page_count {
        return Err(SpaceError::PageIdOutOfRange(page_id));
    }
    let mut buf = vec![0u8; PAGE_SIZE];
    self.file.seek(SeekFrom::Start(Self::offset_of(page_id)))?;
    self.file.read_exact(&mut buf)?;          // io::Error → SpaceError via From
    let page = Page::from_bytes(&buf).map_err(|_| SpaceError::SuperblockInvalidMagic)?;
    Ok(page)
}
```

## Critical Safety Rules

- **Never** skip `sync_all` on a superblock or free-list mutation and still return Ok
- **Never** swallow `FreeListCorrupted` and reset the list — surface the corruption
- **Never** redefine `PAGE_SIZE` to match a corrupted superblock — refuse to open instead
- **Never** write to a page_id >= `page_count` without incrementing `page_count` first (causes PageIdOutOfRange)

## Anti-Patterns

- ❌ `From<std::io::Error>` directly without wrapping in `SpaceError::Io`
- ❌ `panic!` on superblock corruption — return the error so the operator can decide
- ❌ Auto-truncating the file when corruption is detected — silent data loss

## Testing Errors

| Variant | Test scenario |
|---------|---------------|
| `Io(_)` | Point `Space` at a read-only path |
| `PageIdOutOfRange` | Call `read_page(999_999)` on a small file |
| `FreeListCorrupted` | Write a free page with `next = u32::MAX` (v2) |
| `SuperblockInvalidMagic` | Overwrite page 0's magic bytes |
| `SuperblockPageSizeMismatch` | (v2) Write a superblock with `page_size = 4096` |
| `SuperblockVersionUnsupported` | (v2) Write `version = u32::MAX` |
| `SuperblockTruncated` | Open a file < `PAGE_SIZE` |
