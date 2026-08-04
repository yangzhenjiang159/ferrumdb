# Database Guidelines — `ferrumdb-wal`

## File Layout

```
[header: 8B]              next_lsn (u64 LE)
[checkpoint slot: 12B]   magic (u32 LE = 0xFEEDC0DE) + max_flushed_lsn (u64 LE)
[record 0]
[record 1]
...
```

**Fixed checkpoint slot** (bytes 8-19) simplifies open() — no need to scan the whole file looking for the checkpoint. Records always start at offset 20.

## Record Format (Little-Endian)

```
[lsn:         u64 LE]   8 bytes
[page_id:     u32 LE]   4 bytes
[offset:      u32 LE]   4 bytes
[payload_len: u32 LE]   4 bytes
[payload:     N bytes]  variable
[CRC32:       u32 LE]   4 bytes — covers lsn || page_id || offset || payload_len || payload
```

Minimum record size = 24 bytes (empty payload).

## CRC32 Coverage

The 4-byte CRC32 at the end of each record covers the entire record body
**including** the lsn, page_id, offset, and payload_len. Corrupting any single
byte (including the CRC itself) makes the verification fail and `recover` returns
`WalError::RecordCrcMismatch { lsn }`.

## LSN Allocation

- `next_lsn` starts at 1
- Monotonically increasing per `Wal` instance
- Persisted in the 8-byte header; rewritten after every append + fsync
- If a crash interrupts between record-write and header-rewrite, the file's
  header `next_lsn` is one ahead of the actual records. `Wal::open` uses
  `max(header_next_lsn, scanned_next_lsn)` to handle this.

## Append Protocol

```text
1. Compute lsn = self.next_lsn
2. Encode record (lsn, page_id, offset, payload) with CRC32
3. seek to end of file
4. write_all(record bytes)
5. sync_all  ← mandatory; this is the "commit point"
6. self.next_lsn += 1
7. seek to offset 0
8. write_all(next_lsn as 8 bytes)
9. sync_all  ← mandatory; ensures next open sees correct next_lsn
```

If the process is killed between steps 5 and 9, the next open reads the file
header (which still says `next_lsn - 1`) and the scanner finds the records.
The scanner's `scanned_next_lsn` will be correct; the header value is also
recovered via `max()`.

## Checkpoint Protocol

```text
1. seek to offset HEADER_BYTES (8)
2. write_all(CHECKPOINT_MAGIC as 4 bytes)
3. write_all(max_flushed_lsn as 8 bytes)
4. sync_all
```

`max_flushed_lsn` represents "all records with lsn <= this have been flushed to
Space". On recover, records with `lsn > checkpoint_lsn` are replayed; records
with `lsn <= checkpoint_lsn` are skipped (they're already on disk).

## Recovery Protocol

```text
1. Read all bytes from file
2. Read header (next_lsn) and checkpoint slot
3. Scan records from offset 20 to EOF
4. For each record where lsn > checkpoint_lsn:
   a. Call target(record)  ← caller-provided closure
   b. Caller applies the record to Space
5. Truncated tail (incomplete last record) is silently ignored
6. RecordCrcMismatch propagates as error
```

The caller provides a `FnMut(&RedoRecord) -> Result<(), WalError>` closure
that applies each record to the actual storage (typically `Space::write_page`).

## Integration with Space / BufferPool

v1 manual integration:
```rust
let mut space = Space::open(path)?;
let mut wal = Wal::open(wal_path)?;
// On recovery:
wal.recover(|rec| {
    let mut page = space.read_page(rec.page_id)?;
    page.user_data_mut()[rec.offset..rec.offset + rec.payload.len()]
        .copy_from_slice(&rec.payload);
    space.write_page(rec.page_id, &page)?;
    Ok(())
})?;
```

v2 (future): WAL hooks into `BufferPool::fetch_page_mut` automatically.

## Anti-Patterns

- ❌ Skipping `fsync` after `append` — record won't survive crash
- ❌ Skipping header rewrite after `append` — next open may miscount LSNs
- ❌ Appending checkpoint at end of file — breaks with mixed records-after-checkpoint
- ❌ Silently swallowing `RecordCrcMismatch` — caller can't recover
- ❌ Using `String` payload instead of `&[u8]` — defeats zero-copy
- ❌ Trusting header `next_lsn` alone — use `max(header, scanned)` for crash safety
- ❌ Returning `Ok` from `recover` when CRC failed — caller assumes success
