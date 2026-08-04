# Phase 2 Lessons Learned

## What Worked

1. **Row encoding shape** — `null_bitmap + payload` design made decoding trivial because null fields cost 0 bytes. The bitmap is `ceil(n/8)` so even rows with all-null columns encode cheaply.

2. **SlottedPage split into in-memory `records: Vec<u8>` + slot directory** — kept `Page` byte layout unchanged. The 6-byte header (`free_offset`, `free_upper`, `slot_count`) is purely SlottedPage bookkeeping, not page-header bookkeeping.

3. **`Split<K, V>` return type for recursive insert** — clean control flow; root split case is naturally expressed by handling `Some(split)` at the top of `insert()`.

4. **`*mut Node<K, V>` + `PhantomData<Box<Node<K, V>>>` for leaf chain** — avoids `Rc`/`Arc` overhead, keeps `Send + Sync` derivation. Safe because the iter borrows from `&'a BTree`.

## What Bit Me

1. **Offset space confusion in SlottedPage** — `slot.offset` is in `user_data` space (includes 6-byte SlottedPage header), but `self.records` is 0-indexed. Three iterations to get insert/get/from_page/to_bytes consistent. **Lesson**: keep a single offset convention and document it loudly at the struct definition.

2. **`PageError::EncodingError` initial test had a buggy first assert** — wrote `[1]` first then `[0]`; first assert was nonsense leftover from thinking through the bitmap. **Lesson**: read test assertions back to back before commit.

3. **`#![deny(missing_docs)]` failures cascade** — every pub field, method, and struct needs a doc comment. Adding fields late (e.g. `Split.right`) requires updating the doc on the struct. **Lesson**: write the doc at the same time as the type, not after.

4. **`Schema` field additions broke `ferrumdb-engine` test** — extending a public value-type field is API-breaking. The fix was small (update the test's `Schema { columns: vec![] }` literal), but it would have broken real callers. **Lesson**: when phase-N adds fields to a value type, document this in the design as "API-breaking for phase-N+1 callers".

5. **`insert` len increment bug** — counting both overwrite and fresh insert as "+1" inflated `len`. Switched `insert_into` to return `(Option<Split>, bool)` where bool = was_inserted. **Lesson**: invariants on counters should be enforced at the leaf, not the root.

6. **`Node::Leaf { keys, values, next }` pattern destructure missing `_marker`** — added `PhantomData` field means all destructuring patterns must include `_marker` or `..`. **Lesson**: when adding `_marker` PhantomData fields, do a project-wide `cargo check` immediately.

## Patterns Worth Codifying

1. **SlottedPage invariants** (capture in code comments):
   - `free_offset = HEADER_BYTES + records.len()` (always)
   - `free_upper` decreases by `SLOT_BYTES` on each new slot
   - Tombstone = `(0, 0)`

2. **Recursive insert pattern**: when the child splits, the parent must re-balance. Returning `Option<Split>` from the recursive call is the cleanest way to propagate this.

3. **`#![deny(missing_docs)]` workarounds** for `*mut` fields: keep them private; only `pub` items need docs. `next` in `Leaf` is private, so it doesn't need a doc comment.

## Numbers

- **Lines added**: ~1300 (row.rs 344, slotted.rs 364, error.rs 30, node.rs 95, tree.rs 530, lib.rs 25, design + tests)
- **Tests added**: 24 (row 8, slotted 9, btree 8, export check 1, page existing 7 preserved)
- **Failures during development**: 7 (4 in SlottedPage offset confusion, 1 i64 LE test typo, 1 insert len bug, 1 doc comment)
- **Final state**: 0 failures, 0 clippy warnings, all phase-1 tests preserved
