# Database Guidelines — `ferrumdb-engine`

## Trait Contract (current)

Defined in `crates/ferrumdb-engine/src/engine.rs`:

| Method | Phase | Returns |
|--------|-------|---------|
| `create_table(name, schema)` | 6（最小引擎）/ 7（完整） | `Result<(), EngineError>` |
| `drop_table(name)` | 7 | `Result<(), EngineError>` |
| `insert(table, row)` | 6（最小引擎）/ 7（完整） | `Result<(), EngineError>` |
| `update(table, pk, row)` | 7 | `Result<(), EngineError>` |
| `delete(table, pk)` | 7 | `Result<(), EngineError>` |
| `get_by_pk(table, pk)` | 6（最小引擎）/ 7（完整） | `Result<Option<Row>, EngineError>` |
| `scan(table, range)` | 6 | `Result<RowIterator, EngineError>` |
| `create_index(table, meta)` | 6 | `Result<(), EngineError>` |
| `get_by_index(table, index, key)` | 6 | `Result<Option<Row>, EngineError>` |
| `scan_index(table, index, range)` | 6 | `Result<RowIterator, EngineError>` |
| `begin()` | 9 | `Result<TransactionId, EngineError>` |
| `commit(tx)` | 9 | `Result<(), EngineError>` |
| `rollback(tx)` | 9 | `Result<(), EngineError>` |

Each method's doc comment lists its phase. The trait is object-safe — proven
by the test in `crates/ferrumdb-sql/src/lib.rs:14-16`.

## Method Implementation Order

Per `docs/plan.md` 阶段 0 + 阶段 7:

1. `create_table` first (creates catalog entry + empty B+Tree)
2. `insert` / `get_by_pk` together (round-trip is the simplest integration test)
3. `update` / `delete` after insert works
4. `scan` after `get_by_pk` works (range scan needs a populated B+Tree)
5. `drop_table` last (it must clean up catalog + all pages)
6. `begin` / `commit` / `rollback` are phase 9; before then, return `EngineError::Unsupported`

## RangeBound Semantics

Defined in `crates/ferrumdb-engine/src/engine.rs:10-26`:

```rust
pub struct RangeBound {
    pub start: Option<Value>,  // inclusive; None = min
    pub end: Option<Value>,    // exclusive (default); None = max
}
impl RangeBound { pub fn full() -> Self }
```

**Open question** (phase 6 will resolve): are bounds inclusive or exclusive
on `end`? Document the choice in the trait method's doc when implemented.

## RowIterator Lifetime

```rust
pub type RowIterator<'a> = Box<dyn Iterator<Item = Result<Row, EngineError>> + 'a>;
```

`crates/ferrumdb-engine/src/engine.rs:30`. The iterator borrows from the
engine (`'a`). Implementations must NOT require `clone` of internal state.

## Integration Wiring (phase 7)

`FerrumEngine` will hold:

| Field | Type | Purpose |
|-------|------|---------|
| `space` | `Space` | Disk file |
| `buffer` | `BufferPool` | Page cache |
| `wal` | `Wal` | Redo log |
| `btree` | (per table) `BTree` | Storage |
| `catalog` | `TableCatalog` | Schema + root page_ids |
| `txn_state` | `TxnState` | Active txns (phase 9+) |

Dependency direction is **engine holds** the storage crates — never the reverse.

## Anti-Patterns

- ❌ Adding `async` to trait methods (engine is sync)
- ❌ Making the trait non-object-safe (breaks `dyn StorageEngine` usage in `ferrumdb-sql`)
- ❌ Returning `Result<Vec<Row>, _>` from `scan` (large tables will OOM — use `RowIterator`)
- ❌ Adding business logic directly in `engine.rs`
- ❌ Allowing `create_table` of a name that already exists without an error variant
