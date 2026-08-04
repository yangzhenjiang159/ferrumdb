# Quality Guidelines — `ferrumdb-engine`

## Required Patterns

1. **`#![deny(missing_docs)]`** — must be set in `lib.rs` when implementation lands (currently the trait file does not have this attribute, but every `pub` item already has doc comments).

2. **Module `//!` doc** in standard format (Chinese responsibilities + phase 7 reference).

3. **`EngineError` in `error.rs`** (planned split from `engine.rs`); `thiserror` derive.

4. **Trait doc per method** listing:
   - What the method does
   - Which phase implements it
   - `# Errors` section enumerating `EngineError` variants

5. **Object-safe trait**: no generic methods, no `Self` in return types, no `where Self: Sized` on required methods. Proven today by `crates/ferrumdb-sql/src/lib.rs:14-16`.

6. **`RowIterator` returns borrowed iterator**, never materialised `Vec`.

7. **Phase-aware `Unsupported`**: methods not yet implemented in the current phase return `EngineError::Unsupported(...)` with the phase name as the message.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| Adding `async` to the trait | Sync library; `tokio` belongs in `ferrumdb-server` |
| Materialising a scan into `Vec` | Memory blow-up on large tables |
| `Internal(String)` for a known case | Add a specific variant |
| Generic methods | Breaks object safety |
| Logging inside trait methods | Hot path; caller logs |
| Implementing trait methods that ignore their `pk` argument | Defeats the contract |
| `dyn StorageEngine` only via `Box`; `Rc<dyn>` not allowed | `Send + Sync` future-proofing |

## Testing Requirements

When implementation lands:

- [ ] `create_table` + `drop_table` round-trip
- [ ] `insert` + `get_by_pk` round-trip
- [ ] `insert` with duplicate PK returns `DuplicateKey`
- [ ] `get_by_pk` on missing PK returns `Ok(None)` (not `Err`)
- [ ] `get_by_pk` on missing table returns `Err(TableNotFound)`
- [ ] `scan` over a populated table returns all rows in PK order
- [ ] `scan` with `RangeBound::full()` returns all rows
- [ ] `begin` / `commit` / `rollback` return `Unsupported` before phase 9
- [ ] Object-safety test in `ferrumdb-sql` continues to pass
- [ ] End-to-end crash recovery: insert, "kill" process, restart, get_by_pk returns the row

## Code Review Checklist

- [ ] Module `//!` doc references phase 7
- [ ] No new external dep
- [ ] All `pub` items have `///` doc comments
- [ ] Trait method doc comments include `# Errors` sections
- [ ] `EngineError` variants are specific, not stringly-typed
- [ ] `cargo test -p ferrumdb-engine` passes (today only `crate_compiles` smoke test)
- [ ] If `dyn StorageEngine` is used anywhere, the type still satisfies object safety
