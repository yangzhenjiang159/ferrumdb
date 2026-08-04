# Database Guidelines — `ferrumdb-btree`

Phase-2 (in-memory) + phase-3 (persistent) B+Tree conventions.

---

## Constants

```rust
pub const ORDER: usize = 64;       // crates/ferrumdb-btree/src/node.rs:6
pub const MIN_KEYS: usize = 32;    // crates/ferrumdb-btree/src/node.rs:11
```

Split threshold: `keys.len() >= ORDER` triggers split (both in-memory and persistent).

## In-Memory Node Hierarchy (phase 2)

```rust
pub enum Node<K, V> {
    Internal { keys: Vec<K>, children: Vec<Box<Node<K, V>>> },
    Leaf {
        keys: Vec<K>,
        values: Vec<V>,
        next: Option<*mut Node<K, V>>,   // raw ptr + PhantomData<Box<Node<K, V>>>
        _marker: PhantomData<Box<Node<K, V>>>,
    },
}
```

## Persistent Node ↔ Page Layout (phase 3)

Authoritative source: `crates/ferrumdb-btree/src/persist.rs:14-31`.

```
+--------------------+
| kind: u8           |   0=Internal, 1=Leaf
| reserved: 3 bytes  |
| key_count: u16 LE  |   ≤ ORDER
| reserved: 2 bytes  |
+--------------------+   header = 8 bytes
| keys: [len:u32 LE][bytes] * key_count
| (Internal only) children: [count:u32][page_id:u32]*count
| (Leaf only)     values: [len:u32][bytes] * key_count
| (Leaf only)     next_leaf: [is_some:u8][page_id:u32]
+--------------------+
```

Keys and values are stored as length-prefixed byte arrays (matches `ferrumdb-page::encode_row` style for byte-typed columns).

## Insert & Split

### In-Memory (phase 2)

When `keys.len() >= ORDER`:

1. `mid = keys.len() / 2`
2. **Leaf split** (`tree.rs::split_leaf`):
   - `right_keys = keys.split_off(mid)`, `right_values = values.split_off(mid)`
   - Up-pushed key = `right_keys[0].clone()` (boundary key copied to parent)
   - Old leaf's `next` becomes new right's `next`; old leaf's `next` is the new right
3. **Internal split** (`tree.rs::split_internal`):
   - `up_key = keys.remove(mid)` (boundary key moved to parent — not copied)
   - `right_keys = keys.split_off(mid)`, `right_children = children.split_off(mid + 1)`
4. **Root split** (`tree.rs::insert`):
   - New internal root: `keys = [split.key]`, `children = [old_root, split.right]`
   - Tree height increases by 1

### Persistent (phase 3)

Same algorithm, but each new node is `write_page`'d immediately. `PersistentBtree` does NOT cache dirty nodes; every split + every modification writes through.

```rust
if keys.len() >= ORDER {
    let mid = keys.len() / 2;
    let right_keys = keys.split_off(mid);
    let right_values = values.split_off(mid);
    let up_key = right_keys[0].clone();
    
    let right_id = source.allocate_page()?;
    let right_page = build_leaf_page(right_id, &right_keys, &right_values, next_leaf)?;
    source.write_page(right_id, &right_page)?;
    
    let left_page = build_leaf_page(page_id, &keys, &values, Some(right_id))?;
    source.write_page(page_id, &left_page)?;
    
    return Ok(Some(Split { up_key, right_page_id: right_id }));
}
```

## Get (persistent) — B+Tree Correctness

The persistent `get` MUST walk the leaf chain because in B+Tree a separator
key pushed to the parent may not be in the descended-into leaf:

```rust
// Walk leaf chain until found or determined absent
while let Some(pid) = cur {
    let node = decode_node_from_page(&source.read_page(pid)?)?;
    if let DecodedNode::Leaf { keys, values, next_leaf } = node {
        let idx = lower_bound_keys(&keys, key);
        if idx < keys.len() && keys[idx].as_slice() == key {
            return Ok(Some(values[idx].clone()));
        }
        if idx >= keys.len() {
            cur = next_leaf;   // key greater than all in this leaf — try next
            continue;
        }
        return Ok(None);       // key less than keys[idx] in this leaf — absent
    }
}
```

In-memory `BTree::get` doesn't need this because the in-memory `Node::Leaf` always has the right keys (boundary key in the leaf, not duplicated).

## Duplicate Key Handling

- **In-memory `BTree::insert`**: overwrite; `insert_into` returns `(Option<Split>, bool)` where bool = was_inserted; outer code only increments `len` on true.
- **Persistent `PersistentBtree::insert`**: overwrite; `len` is always incremented (v1 simplification tracked as a known issue).

## Range Scan

### In-memory

Returns `ScanIter<'a, K, V>` — borrowing iterator over leaf chain via raw pointers.

### Persistent

`scan_range(start, end)` returns `Vec<(Vec<u8>, Vec<u8>)>` — materialises into a Vec (v1 simplification). Walks the leaf chain starting from the descended-into leaf.

## Generic Constraints

- In-memory: `K: Ord + Clone`, `V: Clone`
- Persistent: K/V are `Vec<u8>` — no generic bounds; callers handle encoding

## Interaction With Other Crates

| Phase | What `ferrumdb-btree` provides | What it must NOT do |
|-------|--------------------------------|---------------------|
| 2 (memory) | `BTree::new`, `insert`, `get`, `delete`, `scan_range`, `scan_all` | Touch disk |
| 3 (persistent) | Above + `PersistentBtree::create`, `open`, `insert`, `get`, `scan_range` + 节点 ↔ Page | Open files; that's `ferrumdb-space`'s job |
| 6 (secondary index) | `(idx_key, pk)` leaf layout | Call back into `ferrumdb-engine` |

## Anti-Patterns

- ❌ Inline magic numbers for split / merge thresholds
- ❌ Materialising a scan into `Vec<(K, V)>` for in-memory (use `ScanIter`)
- ❌ Comparing keys via `==` on `Vec<u8>` in a hot loop
- ❌ Calling back into `ferrumdb-engine` from inside btree code
- ❌ **Persistent `get` not walking the leaf chain** — produces "key not found" for separator keys
- ❌ Mutating leaf chain pointers only on insertion (not on deletion)
- ❌ Returning `Result<Option<T>, E>` where `Result<T, E>` would do
