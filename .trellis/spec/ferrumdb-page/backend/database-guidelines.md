# Database Guidelines — `ferrumdb-page`

On-disk page format + phase-2 row encoding + Slotted Page conventions.

---

## Disk Layout (16 384 bytes)

Authoritative source: `crates/ferrumdb-page/src/page.rs:8-22`.

```
+---------------------------+  offset 0
| PageHeader (32 bytes)     |
+---------------------------+  offset 32
| User Data (16344 bytes)   |
+---------------------------+  offset 16376
| PageFooter (8 bytes)      |
+---------------------------+  offset 16384
```

Constants (`crates/ferrumdb-page/src/page.rs:14-38`):

| Constant | Value | Meaning |
|----------|-------|---------|
| `PAGE_SIZE` | `16384` | Total bytes on disk — never redefined elsewhere |
| `PAGE_HEADER_SIZE` | `32` | Fixed |
| `PAGE_FOOTER_SIZE` | `8` | Reserved for backward-compatible extension |
| `PAGE_USER_DATA_OFFSET` | `PAGE_HEADER_SIZE` | Where user records start |
| `PAGE_FOOTER_OFFSET` | `PAGE_SIZE - PAGE_FOOTER_SIZE` | Where footer starts |
| `PAGE_USER_DATA_SIZE` | `PAGE_SIZE - PAGE_HEADER_SIZE - PAGE_FOOTER_SIZE` | 16344 |
| `PAGE_MAGIC` | `0xFEDB_0001` | Identifies a FerrumDB page |
| `PAGE_HEADER_VERSION` | `1` | Bump when header layout changes |

## Byte Order

**Little-endian everywhere** — `crates/ferrumdb-page/src/page.rs:6` and every codec path.

## Row Encoding (phase 2)

Authoritative source: `crates/ferrumdb-page/src/row.rs:6-32`.

```
+----------------------------+
| null_bitmap (N bytes)      |   N = ceil(column_count / 8); bit i set => column i is NULL
+----------------------------+
| column 0 bytes             |
| column 1 bytes             |
| ...                        |
| column (n-1) bytes         |
+----------------------------+
```

Per-column byte layout:

| `Value` variant | Bytes |
|------------------|-------|
| `Null` | 0 (signaled by bitmap) |
| `I64(i64)` | 8 bytes LE |
| `Bytes(Vec<u8>)` | `[len: u32 LE (4B)][bytes (len B)]` |

`Schema` carries per-column type info (`ColumnType::{I64, Bytes, Any}`); `Any` cannot participate in decoding.

## Slotted Page (phase 2)

Authoritative source: `crates/ferrumdb-page/src/slotted.rs:9-31`.

```
+---------------------------+  offset 0 (within user_data)
| header: free_offset (u16) |
| header: free_upper  (u16) |   <- 6-byte SlottedPage header
| header: slot_count  (u16) |
+---------------------------+  offset 6
| record 0 bytes            |
| record 1 bytes            |
| ...                       |
| (free space)              |
| ...                       |
+---------------------------+
| slot N-1: (off, len)      |   <- 4 bytes per entry, growing down
| slot N-2: (off, len)      |
| ...                       |
| slot 0: (off, len)        |
+---------------------------+  offset PAGE_USER_DATA_SIZE (16344)
```

Key invariants:
- `records` field stores **only** record bytes (no SlottedPage header)
- `free_offset` = `HEADER_BYTES + records.len()` (in user_data space)
- `free_upper` decreases by `SLOT_BYTES` (4) on each new slot
- Tombstone = `SlotEntry { offset: 0, len: 0 }`
- Same-size overwrite = in-place rewrite; different-size = append new slot + new entry

## PageType Convention

`#[repr(u8)]` enums paired with explicit `to_u8` / `from_u8 -> Result<_, PageError>`:

```rust
#[repr(u8)]
pub enum PageType {
    Free = 0,        // free-list page
    Data = 1,        // Slotted Page (phase 2 — implemented)
    Index = 2,       // B+Tree node page (phase 3)
    Superblock = 3,  // tablespace page 0
}
```

Reference: `crates/ferrumdb-page/src/page.rs:60-91`.

**Rules:**

- **Never** `as u8` ad-hoc — always go through `to_u8()` / `from_u8()`
- New variants append at the next free value; never reuse or renumber
- `from_u8` returns `PageError::UnknownPageType(u8)` for unrecognised bytes
- Round-trip test required: `PageType::from_u8(PageType::Data.to_u8()) == Ok(PageType::Data)`

## Checksum Strategy

CRC32 over header + user data, stored in **both** header and footer.

## Row / Value / Schema

Phase-2 schema is extended:

```rust
pub enum ColumnType { I64, Bytes, Any }

pub struct Schema {
    pub columns: Vec<String>,
    pub types: Vec<ColumnType>,
    pub primary_key: Option<usize>,
}
```

`Schema::from_names(...)` is a convenience for tests; production code must provide `types`.

## Key Encoding (phase 6) — `key.rs`

B+Tree keys use a **separate order-preserving encoding** (module `key.rs`,
`encode_key` / `decode_key` / `encode_pk` / `encode_secondary_key` /
`upper_bound` / `successor`). `encode_row` is NOT order-preserving (I64 LE,
Bytes length-prefixed), so it cannot be used as a tree key.

| `Value` | Encoding |
|---------|----------|
| `Null` | `0x00` |
| `I64(v)` | `0x01` ++ `((v as u64) ^ 1<<63)` big-endian (8 B) |
| `Bytes(b)` | `0x02` ++ `0x00`→`0x00 0xFF` escaped ++ `0x00 0x00` terminator |

- **Type tag** (first byte) makes all encodings prefix-free (e.g. `i64::MIN`
  flips to `[0;8]`; without the tag it would collide with `Null`'s `[0x00]`).
- **Big-endian** inside keys (order-preserving) — the little-endian rule above
  applies to page/row codecs, not key ordering.
- Secondary composite key = `index_key_enc ∥ pk_enc`; prefix scans use
  `successor(P)` as the exclusive upper bound for `scan_range(P, successor(P))`.
