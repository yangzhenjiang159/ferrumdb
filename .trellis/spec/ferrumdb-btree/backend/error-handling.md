# Error Handling — `ferrumdb-btree`

## Current Definition (phase 3)

`crates/ferrumdb-btree/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum BTreeError {
    #[error("key not found")]
    KeyNotFound,

    #[error("duplicate key")]
    DuplicateKey,

    #[error("page error: {0}")]
    Page(#[from] ferrumdb_page::PageError),

    #[error("space error: {0}")]
    Space(#[from] ferrumdb_space::SpaceError),

    #[error("comparison failed: {0}")]
    ComparisonFailed(String),

    #[error("invalid node kind: {0}")]
    InvalidNodeKind(u8),

    #[error("node has too many keys: {got} > {max}")]
    TooManyKeys { got: usize, max: usize },

    #[error("node arity mismatch: keys={keys}, children={children}, values={values}")]
    ArityMismatch { keys: usize, children: usize, values: usize },
}
```

**Note**: phase 3 dropped `PartialEq + Eq` because `SpaceError` doesn't implement `Eq` (it wraps `std::io::Error`).

## Conventions

- `thiserror::Error` derive
- `#[from]` on wrapped errors so `?` works
- Specific variants; **no** `Internal(String)`
- `KeyNotFound` / `DuplicateKey` are reserved (current `get` returns `Ok(None)` on miss; `insert` overwrites silently)

## Variant Taxonomy

| Category | Variants |
|----------|----------|
| Reserved | `KeyNotFound`, `DuplicateKey`, `ComparisonFailed` |
| Wrapped page error | `Page(#[from] PageError)` |
| Wrapped space error | `Space(#[from] SpaceError)` |
| Node format | `InvalidNodeKind`, `TooManyKeys`, `ArityMismatch` |

## Propagation Pattern

```rust
fn insert<S: PageSource + ?Sized>(...) -> Result<(), BTreeError> {
    let split_opt = self.insert_into(source, self.root_page_id, key, value)?;
    // ? converts PageError / SpaceError → BTreeError via From
    ...
    Ok(())
}
```

## Anti-Patterns

- ❌ Adding `From<std::io::Error>` — let it propagate through `SpaceError`
- ❌ Adding `Internal(String)` — file an issue to define a new variant
- ❌ Hand-rolled `impl Error`
- ❌ Logging the error inside the tree — propagate up; the caller logs with context

## Testing Errors

| Variant | Test scenario |
|---------|---------------|
| `Page(_)` | Pass truncated bytes to `Node::from_page` (when implemented) |
| `Space(_)` | Inject a `Space` mock that returns `SpaceError` |
| `InvalidNodeKind` | Decode a page with bogus kind byte |
| `TooManyKeys` | Decode a page with `key_count > ORDER` |
| `ArityMismatch` | Encode/decode with mismatched keys vs children vs values |
