# Directory Structure — `ferrumdb-btree`

Real layout (2026-07-18, post phase-3 implementation):

```
crates/ferrumdb-btree/
├── Cargo.toml              # ferrumdb-page + ferrumdb-space + thiserror; dev: tempfile
└── src/
    ├── lib.rs              # Module doc + #![deny(missing_docs)] + re-exports + types export test
    ├── error.rs            # BTreeError enum (thiserror) — phase 3 added Space/InvalidNodeKind/TooManyKeys/ArityMismatch
    ├── node.rs             # Node enum + ORDER + MIN_KEYS + lower_bound (phase 2)
    ├── tree.rs             # In-memory BTree + ScanIter + 8 unit tests (phase 2)
    ├── persist.rs          # Node ↔ Page codec (EncodedNode, DecodedNode, KIND_*) + 4 unit tests — phase 3 NEW
    └── persistent.rs       # PersistentBtree + 4 integration tests (1000-key reopen etc.) — phase 3 NEW
```

## File responsibilities

| File | Purpose | Public surface |
|------|---------|----------------|
| `lib.rs` | Crate-level `//!` doc + `#![deny(missing_docs)]` + `pub use` re-exports | `BTree`, `BTreeError`, `Node`, `MIN_KEYS`, `ORDER`, `ScanIter`, `Split`, `PersistentBtree`, `DecodedNode`, `EncodedNode`, `KIND_INTERNAL`, `KIND_LEAF` |
| `error.rs` | `BTreeError` enum. Phase 3 dropped `PartialEq + Eq` (Space doesn't impl Eq); added Space/InvalidNodeKind/TooManyKeys/ArityMismatch | 9 variants total |
| `node.rs` | `Node<K, V>` + constants + free `lower_bound` | Phase 2 |
| `tree.rs` | In-memory BTree | Phase 2 |
| `persist.rs` | Node ↔ Page binary codec. **Phase 3 NEW**. | `EncodedNode`, `DecodedNode`, `encode_node_to_page`, `decode_node_from_page`, `DecodedNode::load`, `KIND_INTERNAL = 0`, `KIND_LEAF = 1` |
| `persistent.rs` | `PersistentBtree` impl + integration tests. **Phase 3 NEW**. | `PersistentBtree::create`, `open`, `insert`, `get`, `scan_range`, `root_page_id`, `height`, `len`, `is_empty` |

## Visibility conventions

- All inter-module declarations are private: `mod error;`, `mod node;`, `mod persist;`, `mod persistent;`, `mod tree;`
- Everything public is re-exported from `lib.rs`

## Phase-3 Implementation Notes

- **`PersistentBtree` keys/values are `Vec<u8>`** — generic encoding is deferred. Callers (e.g., future `ferrumdb-engine`) will use `ferrumdb_page::encode_row` to produce key/value bytes.
- **`get` walks the leaf chain** — B+Tree correctness requires following `next_leaf` when a key isn't in the descended-into leaf.
- **`insert` returns `Result<(), BTreeError>`** — overwrite increments `len` (acceptable v1 simplification; will be tightened in phase 4 with proper InsertOutcome).
- **All page I/O goes through `PageSource`** — `PersistentBtree` doesn't import `Space` directly; tests could inject a mock.
- **Root page id is stored in the caller's metadata** (typically `Space.superblock.root_page_id`). `PersistentBtree` keeps an in-memory copy.
