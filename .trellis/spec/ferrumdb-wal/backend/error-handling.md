# Error Handling — `ferrumdb-wal`

## Current Definition (phase 5)

`crates/ferrumdb-wal/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum WalError {
    #[error("wal io: {0}")]
    Io(#[from] std::io::Error),

    #[error("record crc32 mismatch at lsn {lsn}")]
    RecordCrcMismatch { lsn: u64 },

    #[error("wal: log file truncated (incomplete record at end)")]
    Truncated,

    #[error("invalid record: {0}")]
    InvalidRecord(String),

    #[error("checkpoint record corrupt")]
    CheckpointCorrupt,

    #[error("lsn exhausted (u64::MAX reached)")]
    LsnExhausted,

    #[error("lsn out of order: expected {expected}, got {got}")]
    OutOfOrder { expected: u64, got: u64 },
}
```

## Conventions

- `thiserror::Error` derive
- `#[from] std::io::Error` for filesystem errors
- Specific variants; **no** `Internal(String)`
- `Truncated` is **NOT an error** in the recovery path (treat as normal EOF)

## Variant Taxonomy

| Category | Variants |
|----------|----------|
| I/O | `Io` |
| File format | `InvalidRecord`, `CheckpointCorrupt` |
| Truncation (not error in recover) | `Truncated` |
| Integrity | `RecordCrcMismatch` |
| Sequence | `OutOfOrder` |
| Lifecycle | `LsnExhausted` |

## Propagation Pattern

```rust
fn recover<F>(&mut self, mut target: F) -> Result<u64, WalError>
where F: FnMut(&RedoRecord) -> Result<(), WalError>,
{
    // ...
    match RedoRecord::decode(&bytes[pos..], Some(expected)) {
        Ok(rec) => { /* apply */ }
        Err(WalError::Truncated) => break,        // normal EOF
        Err(WalError::RecordCrcMismatch { lsn }) => {
            return Err(WalError::RecordCrcMismatch { lsn });  // propagate
        }
        Err(e) => return Err(e),
    }
}
```

## Critical Safety Rules

- **Never** swallow `RecordCrcMismatch` silently — propagate to caller
- **Never** skip fsync after `append` — record won't survive crash
- **Never** treat `Truncated` as error in `recover` — it's normal EOF
- **Never** re-use a recycled LSN (monotonicity is the whole point)

## Anti-Patterns

- ❌ Replacing `RecordCrcMismatch` with `Internal(String)` — loses the lsn context
- ❌ Catching `WalError` and returning `Ok(())` from `recover` — silent failure
- ❌ Logging inside WAL — propagate; caller logs with context

## Testing Errors

| Variant | Test scenario |
|---------|---------------|
| `Io(_)` | Open WAL on read-only file (deferred — needs permission mocking) |
| `RecordCrcMismatch` | `recover_corrupt_crc_returns_error` test |
| `Truncated` | `recover_handles_truncated_tail` test (verifies Ok, not Err) |
| `InvalidRecord` | Open file < DATA_OFFSET (20 bytes) |
| `CheckpointCorrupt` | (deferred — requires manual byte corruption) |
| `LsnExhausted` | Append at `u64::MAX` (impractical to test) |
| `OutOfOrder` | Internal: `RedoRecord::decode` with wrong expected lsn |
