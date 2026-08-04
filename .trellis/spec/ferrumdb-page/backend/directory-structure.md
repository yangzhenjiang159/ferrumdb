# Directory Structure — `ferrumdb-page`

Real layout (2026-07-18, post phase-2 implementation):

```
crates/ferrumdb-page/
├── Cargo.toml              # thiserror + bytes + crc32fast; no internal deps
└── src/
    ├── lib.rs              # Module doc + #![deny(missing_docs)] + pub use re-exports + 1 smoke test
    ├── error.rs            # PageError enum (thiserror) — phase 2 added EncodingError, PageFull, SlotOutOfRange
    ├── page.rs             # Page, PageHeader, PageFooter, PageType + constants + 7 unit tests (unchanged)
    ├── row.rs              # Row, Value, Schema, ColumnType + encode_row/decode_row + 8 unit tests (phase 2)
    └── slotted.rs          # SlottedPage, SlotEntry + 9 unit tests (phase 2 NEW)
```

## File responsibilities

| File | Purpose | Public surface |
|------|---------|----------------|
| `lib.rs` | Crate-level `//!` doc + `#![deny(missing_docs)]`. Re-exports the entire public API via `pub use`. | `PageError`, `Page`, `PageHeader`, `PageFooter`, `PageType`, `Row`, `Value`, `Schema`, `ColumnType`, `encode_row`, `decode_row`, `SlottedPage`, `SlotEntry`, all `PAGE_*` constants |
| `page.rs` | On-disk 16 KB page: header, footer, user-data buffer, CRC32, byte-level encode/decode. | `Page::new`, `to_bytes`, `from_bytes`, `header`, `footer`, `page_id`, `page_type`, `lsn`, `set_lsn`, `user_data`, `user_data_mut` |
| `error.rs` | `PageError` enum. **Phase 2 added**: `EncodingError(String)`, `PageFull`, `SlotOutOfRange(usize)`. | Five variants total |
| `row.rs` | Logical row + cell types + **phase-2 codec**. `Schema` carries `columns`, `types`, `primary_key`. | `Value::{Null, I64(i64), Bytes(Vec<u8>)}`, `Row { values: Vec<Value> }`, `Schema { columns, types, primary_key }`, `ColumnType::{I64, Bytes, Any}`, `encode_row`, `decode_row` |
| `slotted.rs` | **Phase 2 NEW**: Slotted Page in-memory struct with byte-level `to_bytes` / `from_page` round-trip into `Page::user_data`. | `SlottedPage::new`, `insert`, `get`, `delete`, `slot_count`, `free_space`, `to_bytes`, `from_page`, `round_trip`, `SlotEntry` |

## Visibility conventions

- All inter-module declarations are private: `mod error;`, `mod page;`, `mod row;`, `mod slotted;` (`crates/ferrumdb-page/src/lib.rs:7-10`)
- Everything that crosses the crate boundary is re-exported from `lib.rs` via `pub use` — callers never write `ferrumdb_page::row::Row`
- Field visibility is `pub` for value/data structs (`Row.values`, `Schema.columns`, `Schema.types`, `Schema.primary_key`, `PageHeader.page_id`, `SlotEntry.offset`, `SlotEntry.len`) but encapsulated behind getters when invariants matter (`Page.header()`, `Page.page_id()`, `SlottedPage.page_id()`)

## Phase-2 Additions

- `slotted.rs` did not exist in phase 1; added in phase 2 to support Slotted Page layout.
- `row.rs` extended `Schema` with `types` and `primary_key` (breaking change for any caller constructing `Schema { columns: ... }` — must now also provide `types` and `primary_key`).
- `error.rs` extended with three new variants.
- `lib.rs` re-exports updated.
