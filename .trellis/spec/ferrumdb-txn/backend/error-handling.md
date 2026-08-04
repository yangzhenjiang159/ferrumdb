# Error Handling — `ferrumdb-txn`

## Planned Error Type

```rust
// crates/ferrumdb-txn/src/error.rs (planned)
#[derive(Debug, thiserror::Error)]
pub enum TxnError {
    /// Tried to commit a transaction that was already aborted (or vice versa).
    #[error("invalid txn state transition: {0}")]
    InvalidState(&'static str),

    /// Undo log chain is broken — likely a bug, not user input.
    #[error("undo chain broken at txn {0}")]
    UndoChainBroken(TransactionId),

    /// ReadView references a txn_id that the active-txn table doesn't know about.
    #[error("unknown txn id in read view: {0}")]
    UnknownTxnInReadView(TransactionId),

    /// Lock acquisition timed out (when row locks land in phase 9).
    #[error("lock timeout")]
    LockTimeout,

    /// Wrapped `WalError` from the commit path.
    #[error("wal error: {0}")]
    Wal(#[from] WalError),

    /// Wrapped `PageError` from row parsing.
    #[error("page error: {0}")]
    Page(#[from] PageError),
}
```

## Conventions

- `thiserror::Error` derive
- `#[from]` on the wrapped errors so `?` works for WAL and page parsing
- Specific variants; **no** `Internal(String)`
- Does not derive `PartialEq + Eq` (because of wrapped `WalError`)

## Propagation Pattern

```rust
fn commit(&mut self, txn_id: TransactionId) -> Result<(), TxnError> {
    self.state_check(txn_id, TxnState::Active)?;
    self.wal.append(commit_record(txn_id))?;     // WalError → TxnError via From
    self.wal.fsync()?;                           // critical barrier
    self.mark_committed(txn_id);
    Ok(())
}

fn rollback(&mut self, txn_id: TransactionId) -> Result<(), TxnError> {
    self.state_check(txn_id, TxnState::Active)?;
    self.walk_undo(txn_id)?;                      // may fail with UndoChainBroken
    self.mark_aborted(txn_id);
    Ok(())
}
```

## Critical Safety Rules

- **Never** mark a txn Committed without a successful `wal.fsync()`
- **Never** silently ignore `UndoChainBroken` — surface it; the operator decides
- **Never** allow a write after Commit / Rollback — return `InvalidState`
- **Never** reuse a `TransactionId` after the txn terminates

## Anti-Patterns

- ❌ `panic!` on `UndoChainBroken` — return the error
- ❌ Skipping the WAL fsync to "improve throughput"
- ❌ Logging inside commit/rollback — caller decides
- ❌ Catching `WalError` and reporting "commit succeeded" if WAL failed

## Testing Errors

| Variant | Test scenario |
|---------|---------------|
| `InvalidState` | `commit` a txn that was already rolled back |
| `UndoChainBroken` | Corrupt an `UndoRecord::prev` pointer, then rollback |
| `UnknownTxnInReadView` | Construct a `ReadView` referencing a nonexistent txn |
| `LockTimeout` | (phase 9 when locks land) |
| `Wal(_)` | Inject a WAL that fails on `fsync` |
| `Page(_)` | Pass corrupt row bytes through the version-chain walk |
