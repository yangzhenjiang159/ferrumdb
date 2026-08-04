# Implement — 阶段 2 行编码 + Slotted Page + 内存 B+Tree

## 顺序

```
Step 1: Schema 扩展 + Row 编码 (ferrumdb-page)
  └─ Step 2: SlottedPage (ferrumdb-page)
       └─ Step 3: BTreeError (ferrumdb-btree)
            └─ Step 4: Node + BTree (ferrumdb-btree)
                 └─ Step 5: ScanIter (ferrumdb-btree)
                      └─ Step 6: 全 cargo test 通过
                           └─ Step 7: spec 更新 + 任务收尾
```

## Step 1 — Row encoding

- [ ] 1.1 `row.rs`: 扩展 `Schema` 加 `types: Vec<ColumnType>` 和 `primary_key: Option<usize>`
- [ ] 1.2 `row.rs`: 新增 `ColumnType` 枚举（`I64`, `Bytes`, `Any`）
- [ ] 1.3 `row.rs`: 新增 `encode_row(row, schema) -> Vec<u8>`
- [ ] 1.4 `row.rs`: 新增 `decode_row(bytes, schema) -> Result<Row, PageError>`
- [ ] 1.5 `error.rs`: 新增 `PageError::EncodingError(String)` 与 `PageError::PageFull`
- [ ] 1.6 `row.rs` tests: null_bitmap、I64 LE、Bytes、混合列、类型不匹配返回 EncodingError
- [ ] 1.7 `lib.rs`: 重导出 `ColumnType`、`encode_row`、`decode_row`

**Validation**:
```bash
cargo test -p ferrumdb-page row
```

## Step 2 — SlottedPage

- [ ] 2.1 `slotted.rs`: 定义 `SlotEntry { offset: u16, len: u16 }`
- [ ] 2.2 `slotted.rs`: 定义 `SlottedPage { page_id, page_type, slots, free_offset, free_upper }`
- [ ] 2.3 `slotted.rs`: `SlottedPage::new(page_id, page_type) -> Self`
- [ ] 2.4 `slotted.rs`: `insert(slot_id, record: &[u8]) -> Result<(), PageError>`（覆盖或新建）
- [ ] 2.5 `slotted.rs`: `get(slot_id) -> Option<&[u8]>`
- [ ] 2.6 `slotted.rs`: `delete(slot_id) -> Result<(), PageError>`
- [ ] 2.7 `slotted.rs`: `slot_count() -> usize`
- [ ] 2.8 `slotted.rs`: `to_bytes() -> Vec<u8>` 序列化进 `Page::user_data`
- [ ] 2.9 `slotted.rs`: `from_page(page: &Page) -> Result<Self, PageError>`
- [ ] 2.10 `slotted.rs` tests: insert、update、delete + reuse、PageFull
- [ ] 2.11 `lib.rs`: 重导出 `SlottedPage`、`SlotEntry`

**Validation**:
```bash
cargo test -p ferrumdb-page slotted
cargo test -p ferrumdb-page   # 确保阶段 1 测试不退化
```

## Step 3 — BTreeError

- [ ] 3.1 `error.rs`: 定义 `BTreeError { KeyNotFound, DuplicateKey, InvalidNodeKind(u8), Page(#[from] PageError) }`
- [ ] 3.2 `lib.rs`: 重导出 `BTreeError`
- [ ] 3.3 移除现有 `crate_compiles` 测试，替换为：

```rust
#[test]
fn error_variants_are_constructible() {
    assert_eq!(BTreeError::KeyNotFound, BTreeError::KeyNotFound);
}
```

**Validation**:
```bash
cargo test -p ferrumdb-btree
```

## Step 4 — Node + BTree

- [ ] 4.1 `node.rs`: `Node<K, V>` 枚举（`Internal` / `Leaf`）+ `*mut` 链表指针
- [ ] 4.2 `node.rs`: `Node::new_leaf() -> Box<Self>`、`Node::new_internal() -> Box<Self>`
- [ ] 4.3 `tree.rs`: `BTree<K, V>` 结构
- [ ] 4.4 `tree.rs`: `BTree::new() -> Self`
- [ ] 4.5 `tree.rs`: `BTree::insert(&mut self, key: K, value: V) -> Result<(), BTreeError>`
- [ ] 4.6 `tree.rs`: `BTree::get(&self, key: &K) -> Result<Option<&V>, BTreeError>`
- [ ] 4.7 `tree.rs`: `BTree::delete(&mut self, key: &K) -> Result<bool, BTreeError>`（最小实现：找到则移除；下溢可暂不修复）
- [ ] 4.8 `tree.rs`: 私有 `split_node(node) -> Split<K, V>`
- [ ] 4.9 `tree.rs`: 私有 `link_leaf_chain()` 在所有修改后调用
- [ ] 4.10 `tree.rs`: 私有 `first_leaf()` 维护
- [ ] 4.11 `tree.rs` tests: 1000 顺序插入、1000 反序、10000 随机、root split 后高度正确
- [ ] 4.12 `lib.rs`: 重导出 `BTree`、`Node`

**Validation**:
```bash
cargo test -p ferrumdb-btree
```

## Step 5 — ScanIter

- [ ] 5.1 `iter.rs`: `ScanIter<'a, K, V>` 结构
- [ ] 5.2 `iter.rs`: `BTree::scan(&self, range: (Bound<&K>, Bound<&K>)) -> ScanIter<K, V>` 用 `std::ops::Bound`
- [ ] 5.3 `iter.rs` impl `Iterator for ScanIter`
- [ ] 5.4 `iter.rs` tests: 全范围、半开范围、单 key 范围、空范围
- [ ] 5.5 集成测试: 插入 10000 key 后 scan 全范围，结果按升序、数量正确

**Validation**:
```bash
cargo test -p ferrumdb-btree
```

## Step 6 — 全 cargo test

- [ ] 6.1 `cargo build` 无 warning
- [ ] 6.2 `cargo test` 全过；阶段 1 的 8 个 ferrumdb-page 测试不退化
- [ ] 6.3 `cargo clippy -- -D warnings`（如可用）

**Validation**:
```bash
cargo build && cargo test && (cargo clippy -- -D warnings 2>/dev/null || true)
```

## Step 7 — Spec 同步与收尾

- [ ] 7.1 更新 `.trellis/spec/ferrumdb-page/backend/`：把 SlottedPage、ColumnType、encode_row/decode_row 写入 directory-structure.md、database-guidelines.md、error-handling.md
- [ ] 7.2 更新 `.trellis/spec/ferrumdb-btree/backend/`：把 BTree、Node、ScanIter、ORDER 实际值写入
- [ ] 7.3 启动阶段 2 后的学习笔记（如果踩坑）写入 `.trellis/tasks/07-18-phase-2-row-encoding-and-mem-btree/research/`
- [ ] 7.4 `python3 ./.trellis/scripts/task.py finish`
- [ ] 7.5 `python3 ./.trellis/scripts/task.py archive 07-18-phase-2-row-encoding-and-mem-btree`

## Review gates

每步完成后：
- [ ] `cargo test -p <crate>` 通过
- [ ] 没有引入 `unsafe` / `unwrap()` / 新外部依赖
- [ ] `#![deny(missing_docs)]` 不破
- [ ] 新增 pub 项有 `///` doc
- [ ] 错误变体新增时同步更新对应 `error-handling.md`（可推迟到 Step 7 统一更新）

## Rollback

任一步骤出错：
- 该步骤文件改动通过 `git restore <path>` 回滚
- 上一阶段测试应继续通过
- 如类型不兼容（如 `Schema` 新字段破坏了 `ferrumdb-engine` trait 调用），见 design.md §7 缓解方案
