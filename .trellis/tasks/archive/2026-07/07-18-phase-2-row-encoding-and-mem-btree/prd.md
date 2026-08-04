# 阶段 2 — 行编码 + 内存 B+Tree

## Goal

按 `docs/plan.md` 阶段 2 要求，实现：

1. **行编码**（`ferrumdb-page` crate）：
   - `encode_row(row, schema) -> Vec<u8>`
   - `decode_row(bytes, schema) -> Result<Row, PageError>`
   - 支持定长（`Null`, `I64`）+ 变长（`Bytes`）字段
   - 字段定长部分用 little-endian，变长部分长度前缀
2. **Slotted Page**（`ferrumdb-page` crate）：
   - slot directory 放在页尾，记录 `(offset, len)` 对
   - 记录从页头方向增长，slot directory 从页尾方向增长
   - 插入时从空闲空间分配；满则返回新的 `PageError::PageFull` 变体
3. **内存 B+Tree**（`ferrumdb-btree` crate）：
   - `BTree<K, V>` 内存版（不依赖 `Page`）
   - `insert`, `get`, `delete`, `scan` 操作
   - 节点分裂：叶满 → 分裂 + 向上传播；根分裂 → 树增高
   - 叶子双向链表（范围扫描）
   - ORDER 常量 + MIN_KEYS

## Requirements

### R1 — Row 编码（ferrumdb-page）

| ID | 要求 |
|----|------|
| R1.1 | `Value::Null` 编码为 0 字节载荷 |
| R1.2 | `Value::I64(i64)` 编码为 8 字节 little-endian |
| R1.3 | `Value::Bytes(Vec<u8>)` 编码为 `[len:u32 LE][bytes]` |
| R1.4 | `encode_row` 产出 `[null_bitmap][values...]` 格式，null_bitmap 长度 = `ceil(column_count / 8)` |
| R1.5 | `decode_row` 严格校验 `bytes.len()` 与 schema 列数 + 类型匹配 |
| R1.6 | 类型不匹配返回 `PageError::EncodingError(String)` 新变体 |
| R1.7 | `Row::values.len() == Schema::columns.len()` 不变量保持 |

### R2 — Slotted Page（ferrumdb-page）

| ID | 要求 |
|----|------|
| R2.1 | Slot 目录放在页尾，从后往前增长；记录从前往后增长 |
| R2.2 | Slot 条目 = `(offset:u16, len:u16)`，共 4 字节 |
| R2.3 | Slot 数量记在 `Page` 的 footer 之后 / 之外？决定后写明 |
| R2.4 | 满页插入返回 `PageError::PageFull` |
| R2.5 | 删除 slot 时把 `(offset, 0)` 标记为已删除；后续插入可重用 |
| R2.6 | 已存在 slot 的 `insert` 覆盖原 slot（更新行） |
| R2.7 | `SlottedPage` 是 `Page` 之上的薄封装，`Page` 自身不变 |

### R3 — 内存 B+Tree（ferrumdb-btree）

| ID | 要求 |
|----|------|
| R3.1 | `ORDER = 64`，`MIN_KEYS = ORDER / 2 = 32` 常量 |
| R3.2 | `Node::Internal { keys: Vec<K>, children: Vec<Box<Node>> }` |
| R3.3 | `Node::Leaf { keys: Vec<K>, values: Vec<V>, next: Option<Box<Node>>, prev: Weak<...>或 raw ptr>` |
| R3.4 | 叶子内 `keys` 严格升序 |
| R3.5 | `BTree::insert(&mut self, key: K, value: V) -> Result<(), BTreeError>` |
| R3.6 | `BTree::get(&self, key: &K) -> Result<Option<&V>, BTreeError>` |
| R3.7 | `BTree::delete(&mut self, key: &K) -> Result<bool, BTreeError>` |
| R3.8 | `BTree::scan(&self, range: RangeBound) -> Result<RowIter, BTreeError>` 返回借用迭代器 |
| R3.9 | 节点分裂阈值 `keys.len() > ORDER - 1`；分裂点 `mid = keys.len() / 2` |
| R3.10 | 根分裂 → 树增高 |
| R3.11 | 叶子通过 `next` 形成单向链表（v2 可升级为双向） |
| R3.12 | 通用 `K: Ord + Clone`，`V: Clone` |

### R4 — 错误处理

- 新增 `PageError::EncodingError(String)`、`PageError::PageFull`
- 新增 `BTreeError::KeyNotFound`、`BTreeError::DuplicateKey`、`BTreeError::InvalidNodeKind(u8)`
- 都用 `thiserror` derive

### R5 — 测试

- `cargo test -p ferrumdb-page` 全过 + 新增 row/slotted-page 测试 ≥ 8 个
- `cargo test -p ferrumdb-btree` 全过 + 新增 btree 测试 ≥ 8 个
- 10000 个随机 key 插入 + 全量扫描结果有序

## Acceptance Criteria

- [x] R1: 定长 + 变长 + NULL round-trip 通过测试
- [x] R2: 满页插入返回 `PageFull`；删除后重用；更新覆盖
- [x] R3: 插入 10000 随机 key 后所有 key 均可查询
- [x] R3: 范围扫描结果有序（含 start/end 边界）
- [x] R3: 触发根分裂后树高 +1
- [x] R4: 每个错误变体可达自测试（编码、容量、slot 三类覆盖）
- [x] R5: `cargo build` 无 warning；`cargo test` 全过；`cargo clippy` 0 警告
- [x] 不破坏现有阶段 1（page header/checksum）的 7 个测试
- [x] `#![deny(missing_docs)]` 仍启用，所有 pub 项有 `///` 文档
- [x] spec 文件中已计划的 API 形状与最终实现一致；已更新 `directory-structure.md` / `database-guidelines.md` / `error-handling.md` / `quality-guidelines.md`（page + btree 两个 crate）

## Constraints

- 保持 `Page` 字节布局向后兼容（`PAGE_SIZE`, `PAGE_HEADER_SIZE`, `PAGE_FOOTER_SIZE` 不变）
- 不引入 `unsafe`
- 不引入新外部依赖（workspace 已有的 `thiserror`、`bytes` 够用）
- 不引入 `async`
- 不修改 `ferrumdb-engine`、`ferrumdb-buffer`、`ferrumdb-wal`、`ferrumdb-space`、`ferrumdb-txn`、`ferrumdb-protocol`、`ferrumdb-sql`、`ferrumdb-server`
- 内存 B+Tree 不接触 `ferrumdb-page::Page`（持久化是阶段 3 的事）

## Out of Scope

- 持久化 B+Tree（阶段 3）
- 二级索引与回表（阶段 6）
- `BTreeMap` 替代实现；本任务是手写 B+Tree 教学版
- `RowIterator` 适配 `ferrumdb-engine::RowIterator`（那是阶段 7）
- 并发 / 多线程 B+Tree

## References

- `docs/plan.md` 阶段 2
- `.trellis/spec/ferrumdb-page/backend/database-guidelines.md`
- `.trellis/spec/ferrumdb-page/backend/error-handling.md`
- `.trellis/spec/ferrumdb-btree/backend/database-guidelines.md`
- `.trellis/spec/ferrumdb-btree/backend/error-handling.md`
