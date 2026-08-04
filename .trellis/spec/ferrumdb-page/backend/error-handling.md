# Error Handling — `ferrumdb-page`

The crate has one error type: `PageError`. **Phase 2 added three new variants.**

---

## Current Definition

`crates/ferrumdb-page/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PageError {
    #[error("invalid page length: expected 16384, got {0}")]
    InvalidLength(usize),

    #[error("invalid page magic")]
    InvalidMagic,

    #[error("checksum mismatch")]
    ChecksumMismatch,

    #[error("unknown page type: {0}")]
    UnknownPageType(u8),

    // Phase 2 additions:
    #[error("encoding error: {0}")]
    EncodingError(String),

    #[error("page full")]
    PageFull,

    #[error("slot id out of range: {0}")]
    SlotOutOfRange(usize),
}
```

## Conventions

- `thiserror::Error` derive
- `PartialEq + Eq` so error variants can be used in `assert_eq!` inside tests
- Variant messages are short Chinese strings with `{0}` placeholders for payload fields
- No `From<std::io::Error>` — this crate never touches I/O
- New variants follow the existing pattern: enum tuple variant + `#[error("...")]`

## Variant Taxonomy (phase 2)

| Category | Variants | When |
|----------|----------|------|
| Page format | `InvalidLength`, `InvalidMagic`, `ChecksumMismatch`, `UnknownPageType` | Phase 1 |
| Encoding | `EncodingError(String)` | Phase 2 — arity mismatch, type mismatch, truncated input, NULL bitmap error |
| Capacity | `PageFull` | Phase 2 — SlottedPage insert exceeds free space |
| Index | `SlotOutOfRange(usize)` | Phase 2 — accessing slot_id >= slot_count |

## Propagation Pattern

`?` propagation across the crate. Example:

```rust
pub fn decode_row(bytes: &[u8], schema: &Schema) -> Result<Row, PageError> {
    if bytes.len() < bitmap_len {
        return Err(PageError::EncodingError("row bytes too short".into()));
    }
    // ...
}
```

## Testing Errors (phase 2 examples)

```rust
assert!(matches!(
    encode_row(&row_with_arity_mismatch, &schema),
    Err(PageError::EncodingError(_))
));

let huge = vec![0u8; PAGE_USER_DATA_SIZE];
assert!(matches!(
    slotted_page.insert(0, &huge),
    Err(PageError::PageFull)
));
```

## Anti-Patterns

- ❌ Adding `From<std::io::Error>` "just in case" — this crate is pure CPU work
- ❌ `Internal(String)` catch-all — be specific; the seven variants cover every realistic failure today
- ❌ Replacing `#[error("...")]` with manual `impl Display`
- ❌ Adding `Backtrace` / `anyhow::Error` chains — keep the type simple
