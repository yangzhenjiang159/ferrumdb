# Design — 阶段 2 行编码 + Slotted Page + 内存 B+Tree

## 1. Row 编码格式

```
Row encoding (little-endian):
+---------------------+
| null_bitmap (N B)   |   N = ceil(column_count / 8); bit i = 1 means column i is NULL
+---------------------+
| column 0            |
| column 1            |
| ...                 |
| column (n-1)        |
+---------------------+

Column encoding by Value type:
  Null    -> 0 bytes (signalled by null_bitmap)
  I64     -> 8 bytes LE
  Bytes   -> [len: u32 LE (4B)][bytes (len B)]
```

**为什么不把 NULL 编码进字节流**：bitmap 让定长列可以零拷贝跳过，简化 decoding。

### 1.1 编码流程

```rust
pub fn encode_row(row: &Row, schema: &Schema) -> Vec<u8> {
    assert_eq!(row.values.len(), schema.columns.len());
    let mut buf = Vec::new();
    let bitmap_len = (schema.columns.len() + 7) / 8;
    let mut bitmap = vec![0u8; bitmap_len];
    for (i, v) in row.values.iter().enumerate() {
        match v {
            Value::Null => bitmap[i / 8] |= 1 << (i % 8),
            Value::I64(n) => buf.extend_from_slice(&n.to_le_bytes()),
            Value::Bytes(b) => {
                buf.extend_from_slice(&(b.len() as u32).to_le_bytes());
                buf.extend_from_slice(b);
            }
        }
    }
    bitmap.into_iter().chain(buf).collect()
}
```

### 1.2 解码流程

```rust
pub fn decode_row(bytes: &[u8], schema: &Schema) -> Result<Row, PageError> {
    let bitmap_len = (schema.columns.len() + 7) / 8;
    if bytes.len() < bitmap_len {
        return Err(PageError::EncodingError("row too short for bitmap".into()));
    }
    let (bitmap, mut rest) = bytes.split_at(bitmap_len);
    let mut values = Vec::with_capacity(schema.columns.len());
    for i in 0..schema.columns.len() {
        if bitmap[i / 8] & (1 << (i % 8)) != 0 {
            values.push(Value::Null);
        } else {
            // Schema-aware decode — but we don't have type info in Schema yet (phase 1 stub).
            // Use a heuristic: 8-byte I64 if remaining >= 8 and next 4 bytes aren't a plausible len.
            // BETTER: extend Schema in phase 2 to carry types.
        }
    }
    Ok(Row { values })
}
```

**类型决策**：阶段 1 的 `Schema` 只有 `columns: Vec<String>`，没有列类型。**阶段 2 必须扩展**：

```rust
pub enum ColumnType { I64, Bytes, Any }

pub struct Schema {
    pub columns: Vec<String>,
    pub types: Vec<ColumnType>,
    pub primary_key: Option<usize>,  // column index
}
```

向后兼容：现有 `Row { values: Vec<Value> }` 不变；`Schema` 字段新增，所以 `crates/ferrumdb-engine/src/engine.rs:33` 的 trait 需要更新 `Schema` 类型时不会破坏（trait 方法仍接收 `Schema`）。

## 2. Slotted Page 布局

```
+----------------------+  offset 0
| PageHeader (32 B)    |
+----------------------+  offset 32
| free space pointer   |  -> grows up
|   (slot 0 data)      |
|   (slot 1 data)      |
|     ...              |
|   (free space)       |
|     ...              |
|   (slot N-1 data)    |
+----------------------+
| Slot Directory       |  -> grows down
|   slot 0: (off, len) |  4 bytes each
|   slot 1: (off, len) |
|     ...              |
|   slot N-1           |
+----------------------+
| PageFooter (8 B)     |
+----------------------+  offset 16384
```

Slot 计数 = `(slot_dir_size) / 4` —— 但 page 自身的 `PageFooter` 是固定的 8 字节，不携带 slot 计数。决定：

**方案 A**：在 slot 目录与 footer 之间再放一个 `slot_count: u16`（占 2 字节，footer 之前）。这样 footer 仍在最末尾。
- 优点：footer 格式不变
- 缺点：user data 上限减少 2 字节（16342 字节）

**方案 B**：在 `Page` 之上加 `SlottedPage` 内存结构，自己持有 `Vec<SlotEntry>`，序列化时把目录写进 user_data。
- 优点：`Page` 字节格式不变
- 缺点：内存与磁盘两次表示

**决策**：选 **方案 B**。理由：
1. 阶段 1 的 `Page` 字节布局契约（footer 含 magic + checksum）已稳定，不应改动
2. `SlottedPage` 是内存对象，序列化走 `to_bytes() -> Vec<u8>`，反序列化走 `from_bytes(&[u8]) -> Result<Self>`
3. 阶段 3 持久化时复用同一序列化路径

```rust
pub struct SlottedPage {
    page_id: u32,
    page_type: PageType,
    slots: Vec<SlotEntry>,        // index = slot id
    free_offset: u16,             // next free byte from page head
    free_upper: u16,              // slot dir grows down from here
}

pub struct SlotEntry {
    offset: u16,                  // start of record in user_data; 0 = tombstone
    len: u16,                     // record length; 0 = deleted
}
```

序列化进 `Page` 的 `user_data`：

```
[free_offset: u16 LE][free_upper: u16 LE][slot_count: u16 LE]
[record 0 bytes][record 1 bytes]...[record N-1 bytes]
[slot 0: (off, len)][slot 1: (off, len)]...[slot N-1: (off, len)]
```

固定头部 6 字节 + slot 目录 + 记录。`Page` 自身的 header/footer/magic/checksum 不变。

## 3. 内存 B+Tree 数据结构

```rust
pub const ORDER: usize = 64;
pub const MIN_KEYS: usize = ORDER / 2;

pub enum Node<K, V> {
    Internal {
        keys: Vec<K>,
        children: Vec<Box<Node<K, V>>>,
    },
    Leaf {
        keys: Vec<K>,
        values: Vec<V>,
        next: Option<Box<Node<K, V>>>,
        prev: Option<*mut Node<K, V>>,  // raw pointer with PhantomData; for backward scan if needed
    },
}

pub struct BTree<K, V> {
    root: Option<Box<Node<K, V>>>,
    first_leaf: Option<*mut Node<K, V>>,  // head of leaf chain
    _marker: PhantomData<Box<Node<K, V>>>,
}
```

**叶子链表指针选择**：用 `*mut` 裸指针 + `PhantomData` 而非 `Rc`/`Arc`，理由：
- `Rc` 不允许 `&mut` 借用与不可变借用共存
- `Arc` 引入线程安全开销
- 裸指针带 `PhantomData<Box<Node<K, V>>>` 让 BTree 自身保持 `Send + Sync`（当 `K: Send + Sync, V: Send + Sync`）
- 安全约束：`prev`/`next` 指针只在 BTree 内部使用，不会逃逸

### 3.1 插入流程

```text
fn insert(node, key, value) -> Option<Split<K, V>> {
    // 1. find child index by binary search
    // 2. recurse into child
    // 3. if child returns Some(split), insert split.key at this node
    //    if this node overflows, split again and return
    // 4. if leaf: insert into keys/values; if overflow, split
}
```

`Split { key: K, right: Box<Node<K, V>> }` — 上推的 key 是右子树的最小键。

### 3.2 根分裂

```text
fn insert_root(...) {
    let split = insert(self.root, key, value);
    if let Some(s) = split {
        let new_root = Internal { keys: vec![s.key], children: vec![old_root, s.right] };
        self.root = Some(Box::new(new_root));
    }
}
```

### 3.3 范围扫描

返回借用迭代器：

```rust
pub struct ScanIter<'a, K, V> {
    current: Option<*const Node<K, V>>,
    index: usize,
    range: RangeBound,  // or generic start/end
    _marker: PhantomData<&'a Node<K, V>>,
}

impl<'a, K, V> Iterator for ScanIter<'a, K, V>
where K: Ord + Clone, V: Clone {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        // walk leaf chain, return next (key, value) in range
    }
}
```

## 4. 错误类型

### 4.1 新增 PageError 变体

```rust
pub enum PageError {
    // ... existing variants
    #[error("encoding error: {0}")]
    EncodingError(String),

    #[error("page full")]
    PageFull,
}
```

### 4.2 新增 BTreeError 变体

```rust
pub enum BTreeError {
    #[error("key not found")]
    KeyNotFound,
    #[error("duplicate key")]
    DuplicateKey,
    #[error("invalid node kind: {0}")]
    InvalidNodeKind(u8),
    // planned variants from spec also implemented
}
```

## 5. 测试策略

| 套件 | 关键测试 |
|------|----------|
| `row::tests` | null_bitmap round-trip, I64 LE round-trip, Bytes round-trip, mixed types, type-mismatch error |
| `slotted::tests` | insert + get, update, delete + reuse, full-page returns PageFull |
| `btree::tests` | insert + get, 10k random keys, range scan, root split, leaf chain walk |

## 6. 文件改动清单

```
crates/ferrumdb-page/src/
├── lib.rs              (re-export new types)
├── error.rs            (+ EncodingError, PageFull variants)
├── row.rs              (extend Schema with ColumnType; add encode_row/decode_row)
├── page.rs             (unchanged)
└── slotted.rs          (NEW: SlottedPage + SlotEntry + 6-byte header + tests)

crates/ferrumdb-btree/src/
├── lib.rs              (re-export new types)
├── error.rs            (NEW: BTreeError)
├── node.rs             (NEW: Node enum + Internal/Leaf)
├── tree.rs             (NEW: BTree struct)
└── iter.rs             (NEW: ScanIter)
```

## 7. 风险与回滚

- **风险**：扩展 `Schema` 字段会改变 `ferrumdb-engine::StorageEngine::create_table` 签名
- **缓解**：`Schema` 是值类型，调用方构造时已带上新字段；现无调用方（trait 是 phase 0 stub）
- **回滚**：如真破坏 trait，撤销 `Schema` 新增字段并把类型信息放进 `Row` 侧
