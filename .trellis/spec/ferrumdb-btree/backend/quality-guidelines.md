# Quality Guidelines — `ferrumdb-btree`

## Required Patterns

1. **`#![deny(missing_docs)]`** — set in `crates/ferrumdb-btree/src/lib.rs:11`. Every new `pub` item must have `///` doc comment.

2. **Module-level `//!` doc** following the standard template (Chinese responsibilities + phase reference).

3. **Error type in `error.rs`**, not inlined into `lib.rs`. Use `thiserror`. **Phase 3 dropped `PartialEq + Eq`** because `SpaceError` doesn't impl `Eq`.

4. **Constants for tunable parameters**: `ORDER = 64`, `MIN_KEYS = 32` (`crates/ferrumdb-btree/src/node.rs:6,11`). Never inline magic numbers.

5. **Generic constraints explicit at the impl-block level**:
   ```rust
   impl<K: Ord + Clone, V: Clone> BTree<K, V> { ... }   // in-memory
   impl PersistentBtree { ... }                            // persistent (Vec<u8> only)
   ```

6. **Iterators return `impl Iterator` / `ScanIter`** (in-memory) or `Vec<...>` (persistent v1). Never `Vec<(K, V)>` for in-memory range scans.

7. **Leaf-chain pointers updated on every structural change** — `next` (in-memory) and `next_leaf` (persistent) must be set on every insert split.

8. **Persistent `get` MUST walk the leaf chain** — B+Tree correctness requires following `next_leaf` when the descended-into leaf doesn't contain the key.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| Inline magic numbers for split / merge thresholds | Hides the tuning surface |
| Materialising a scan into `Vec<(K, V)>` for in-memory | Large tables will OOM |
| Manual `split_at` + index juggling instead of `Vec::split_off` | Bug-prone |
| Calling back into `ferrumdb-engine` from inside btree code | Layering violation |
| Persistent `get` not walking the leaf chain | B+Tree correctness violation |
| Mutating leaf chain pointers only on insertion (not on deletion) | Forward pointers break after merge |
| Returning `Result<Option<T>, E>` where `Result<T, E>` would do | Confusing |
| Direct `Space::read_page`/`write_page` in PersistentBtree | Bypass `PageSource` abstraction |
| `unsafe` | Not needed |

## Testing Requirements (phase 3 baseline)

| Method | Minimum tests |
|--------|---------------|
| `BTree::insert` | 1) small tree, 2) 10 000 random keys, 3) root split, 4) overwrite existing key |
| `BTree::get` | 1) present key, 2) absent key returns `Ok(None)`, 3) get on empty tree |
| `BTree::delete` | 1) leaf deletion, 2) deletion on absent key returns `Ok(false)` |
| `BTree::scan_range` | 1) empty range, 2) full range, 3) half-open range, 4) single-key range |
| `persist::encode_node_to_page` / `decode_node_from_page` | leaf + internal round-trip + arity mismatch + invalid kind |
| `PersistentBtree::create` + `insert` + `get` | round-trip with tempdir |
| `PersistentBtree::open` after `drop` | **1000 random keys verified after reopen** (must include separator-key lookup) |
| `PersistentBtree` root split | trigger > 64 inserts; reopen; check height + a few keys |
| `PersistentBtree::scan_range` | persisted scan across multiple leaves |

## Code Review Checklist

- [ ] No new external dep beyond `ferrumdb-page`, `ferrumdb-space`, `thiserror` (+ tempfile dev)
- [ ] All split / merge branches have a test
- [ ] Range scan boundary conditions tested
- [ ] No `unwrap()` / `expect()` in non-test code
- [ ] Module-level `//!` doc references the correct phase number
- [ ] `cargo test -p ferrumdb-btree` passes
- [ ] Leaf-chain pointers (`next_leaf`) updated consistently on every structural change
- [ ] Persistent `get` walks the leaf chain (verified by separator-key lookup test)
