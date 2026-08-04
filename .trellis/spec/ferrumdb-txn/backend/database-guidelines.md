# Database Guidelines — `ferrumdb-txn`

## TransactionId

```rust
pub type TransactionId = u64;
```

Already defined as a type alias in `crates/ferrumdb-engine/src/engine.rs:6`.
This crate uses the same definition; do not create a competing newtype.

Allocation: a global atomic counter `next_txn_id: AtomicU64` — same pattern as
`ferrumdb-wal::next_lsn`.

## Transaction State Machine

```
Inactive -> Active -> { Committed, Aborted }
```

```rust
pub enum TxnState {
    Active,
    Committed,
    Aborted,
}
```

Rules:
- Once `Committed` or `Aborted`, no further operations on the txn
- `commit` requires all writes to be flushed to WAL **before** marking Committed
- `rollback` walks the undo chain in reverse insertion order

## Undo Log

`UndoRecord` (planned):

```rust
pub struct UndoRecord {
    pub txn_id: TransactionId,
    pub table: String,
    pub pk: Value,
    pub kind: UndoKind,        // Insert | Update | Delete
    pub before: Option<Row>,   // old row for Update / Delete
    pub prev: Option<UndoPtr>, // chain pointer
}

pub enum UndoKind { Insert, Update, Delete }
```

- Written **before** the page mutation, so a crash mid-update can always roll back
- Stored inside the buffer pool's page or a dedicated undo segment (TBD)
- The `prev` pointer links an update to its prior undo record — the version chain MVCC walks

## Commit Protocol (per `docs/plan.md` 阶段 9)

```text
1. Append <COMMIT, txn_id> to WAL
2. wal.fsync()
3. Mark txn as Committed (in-memory + persist to txn table)
```

Step 2 is mandatory. A commit without fsync can be lost on power failure.

## Rollback

Walk `UndoRecord::prev` from the txn's most recent record back to the start,
applying each `before` value to the row.

## ReadView (MVCC snapshot)

```rust
pub struct ReadView {
    pub creator_txn_id: TransactionId,
    pub active_txn_ids: Vec<TransactionId>,   // active at snapshot time
    pub min_active: TransactionId,
    pub max_active: TransactionId,
}
```

Visibility rule for row version with `trx_id = X`:

- If `X == creator_txn_id` → visible (own writes)
- If `X < min_active` and X committed → visible
- If `X > max_active` → invisible (didn't exist at snapshot time)
- If `X in active_txn_ids` → invisible (uncommitted)
- Else → visible

## Row Versioning

Once phase 10 lands, rows gain:

```rust
pub struct RowHeader {
    pub trx_id: TransactionId,   // who wrote this version
    pub roll_ptr: Option<UndoPtr>, // pointer to previous version
}
```

This is stored **inside the row encoding** (phase-2 slotted page + phase-10 row header).

## Isolation Level

Phase 10 picks one level — recommended starting point per `docs/plan.md`:

> 阶段 10 MVCC ... 建议先 **Read Committed** 或简化 **RR**

`ferrumdb-txn` owns the `IsolationLevel` enum; the engine is responsible for
actually using it to gate reads.

## Anti-Patterns

- ❌ Skipping WAL fsync before marking Committed
- ❌ Allocating two `TransactionId`s for the same logical txn
- ❌ Allowing writes after Commit / Rollback
- ❌ Forgetting to update `ReadView` when an `Active` txn becomes `Committed`
- ❌ Using `Rc<Transaction>` (must be `Send + Sync` for multi-session later)
