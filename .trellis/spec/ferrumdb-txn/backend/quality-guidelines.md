# Quality Guidelines — `ferrumdb-txn`

## Required Patterns

1. **`#![deny(missing_docs)]`** — set in `crates/ferrumdb-txn/src/lib.rs:14`.

2. **Module `//!` doc** in standard format (Chinese responsibilities + phase 9 + phase 10 references).

3. **Error type in `error.rs`**, `thiserror` derive.

4. **`TransactionId = u64`** — uses the same type alias as `ferrumdb-engine` (`crates/ferrumdb-engine/src/engine.rs:6`); do not redefine.

5. **Undo log written before page mutation** — never the reverse.

6. **WAL fsync before marking Committed** — non-negotiable.

7. **State machine enforcement**: writes after Commit/Abort return `InvalidState`.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| Marking Committed before WAL fsync | Durability violation |
| Skipping the undo log write before mutation | Rollback becomes impossible |
| Using `Rc<Transaction>` | Future multi-session needs `Send + Sync` |
| Logging inside the version-chain walk | Hot path |
| `panic!` on `UndoChainBroken` | Return the error |
| Reusing `TransactionId` after termination | Monotonicity required |
| Holding a `Transaction` borrow across `await` | This crate is sync |

## Testing Requirements

When implementation lands:

- [ ] `begin` / `commit` / `rollback` happy path round-trip
- [ ] `commit` after WAL fsync failure returns error and does not mark Committed
- [ ] Rollback after a sequence of inserts: rollback restores prior state
- [ ] Snapshot read test: txn A inserts uncommitted, txn B (started before A commits) does not see it
- [ ] Snapshot read test: txn A commits, fresh txn C sees the row
- [ ] `UndoChainBroken` test: corrupt the chain, rollback returns error
- [ ] Write-after-commit test: returns `InvalidState`
- [ ] `TransactionId` monotonicity test: 1000 sequential `begin`s get distinct ids

## Code Review Checklist

- [ ] Module `//!` doc references phase 9 (and phase 10 once MVCC lands)
- [ ] No new external dep
- [ ] All `pub` items have `///` doc comments
- [ ] Commit protocol order is correct (append → fsync → mark)
- [ ] Undo log write precedes page mutation
- [ ] `cargo test -p ferrumdb-txn` passes
