# Error Handling — `ferrumdb-engine`

The crate has one error type, `EngineError`. It is defined today in
`crates/ferrumdb-engine/src/engine.rs:33-44` and is the public contract for
every storage failure a caller can observe.

---

## Current Definition

```rust
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("table not found: {0}")]
    TableNotFound(String),

    #[error("duplicate key")]
    DuplicateKey,

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("internal: {0}")]
    Internal(String),
}
```

## Variant Taxonomy

| Category | Variants | Purpose |
|----------|----------|---------|
| User-visible | `TableNotFound`, `DuplicateKey` | Caller can react (different table, retry with new key) |
| Not-yet-implemented | `Unsupported(String)` | Phase 9 methods return this until phase 9 lands |
| System-level | `Internal(String)` | I/O, corruption, assertion failures |

## Conventions

- `thiserror::Error` derive (verified at `crates/ferrumdb-engine/src/engine.rs:33`)
- Chinese error messages, short with `{0}` placeholders
- `TableNotFound` carries the table name; `Unsupported` and `Internal` carry a free-form string
- `DuplicateKey` is **unit** — no payload (the row that violated is already in scope at the call site)
- No `PartialEq + Eq` today; consider adding once `Internal(String)` is gone in favour of specific variants

## Planned Extensions

When phase 7 implements `insert` / `update` / `delete`:

| New variant | When |
|-------------|------|
| `SchemaMismatch { expected, actual }` | DML on a table whose schema doesn't match the row |
| `Wal(WalError)` | WAL failures during commit |
| `Buffer(BufferError)` | Buffer pool failures (via `#[from]`) |
| `BTree(BTreeError)` | B+Tree failures (via `#[from]`) |
| `Txn(TxnError)` | Transaction failures (phase 9+) |

Add new variants instead of overloading `Internal(String)`.

## Propagation Pattern

```rust
fn insert(&mut self, table: &str, row: Row) -> Result<(), EngineError> {
    let schema = self.catalog.lookup(table)
        .ok_or_else(|| EngineError::TableNotFound(table.to_string()))?;
    schema.validate(&row)?;
    self.buffer.pin_mut(root_page_id)?           // BufferError → EngineError via From
        .insert(row)?;
    Ok(())
}
```

Once `#[from]` is wired, callers can use `?` freely.

## Critical Safety Rules

- **Never** use `Internal(String)` for a known case — add a specific variant
- **Never** swallow a `Wal` / `Buffer` / `BTree` error and convert to `Internal`
- **Never** return `Ok` from an operation that may have left the engine in an inconsistent state

## Anti-Patterns

- ❌ Replacing a specific variant with `Internal(format!("table not found: {}", name))`
- ❌ Logging inside the engine — propagate; the caller decides
- ❌ Using `panic!` instead of `Unsupported` for not-yet-implemented features

## Testing Errors

The `EngineError` variants are reachable through:

| Variant | Test scenario |
|---------|---------------|
| `TableNotFound` | `get_by_pk` on a non-existent table |
| `DuplicateKey` | Two `insert`s with the same primary key |
| `Unsupported` | Calling `begin` before phase 9 |
| `Internal` | Inject a fault in the underlying `Space` / `Buffer` / `Wal` mock |
