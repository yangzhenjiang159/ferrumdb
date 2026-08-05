# 阶段 6：二级索引与回表 — 技术设计

## 1. 架构与边界

```
ferrumdb-page (key 保序编码)  →  ferrumdb-btree (无改动, byte key)  →  ferrumdb-engine (FerrumEngine 编排 + 内存 catalog)
```

- `ferrumdb-btree` **零改动**：`PersistentBtree` 已按 `(Vec<u8>, Vec<u8>)` 泛型工作（create/open/get/insert/scan_range），满足二级索引一切需求。遵守 spec 约束（btree 不依赖 buffer/wal/space）。
- 跨层接线（多树编排、回表、唯一检查）全部落在 `ferrumdb-engine`。
- `ferrumdb-engine` 现有 `StorageEngine` trait 保持 object-safe，新增 3 个方法 + `IndexMeta` 类型。

## 2. 数据结构

```rust
// ferrumdb-engine
pub struct IndexMeta {
    pub name: String,          // 索引名，get_by_index / scan_index 用
    pub columns: Vec<usize>,   // 引用 Schema::columns 的列下标（非空、越界校验）
    pub is_unique: bool,
}

struct TableMeta {
    name: String,
    schema: Schema,
    clustered_root: u32,                // 聚簇 PersistentBtree root
    indexes: Vec<IndexEntry>,           // N 个二级索引
}
struct IndexEntry { meta: IndexMeta, root: u32 }

pub struct FerrumEngine {
    space: Space,                       // 单线程 &mut 使用；PageSource 直连（见 §6 权衡）
    catalog: HashMap<String, TableMeta>,
}
```

- 根分裂时 `PersistentBtree::insert` 会改变 `root_page_id`，insert 后必须把 `tree.root_page_id()` 回写 `TableMeta`（聚簇 + 各二级）。

## 3. 键编码（ferrumdb-page 新增 `key` 模块）

现状：`encode_row`（row.rs）**不保序**（I64 用 LE、Bytes 用长度前缀，字节序≠值序），不能直接做 B+Tree key。新增**保序编码**：

- `Value` → 保序字节：
  - `Null` → `0x00`
  - `I64(v)` → `(v as u64) ^ (1 << 63)` 的 **8 字节 big-endian**（符号位翻转后大端即保序）
  - `Bytes(b)` → 0x00-escape 变长：每字节 `0x00`→`0x00 0xFF`，其余原样；结尾 `0x00 0x00`（单射、无歧义）
- 聚簇 key = 主键列的保序字节（`encode_pk`）。
- 二级 key = 各索引列的保序字节拼接（各列自定界，无歧义）`∥ 0x00 0x00 ∥ pk_bytes`（分隔符与列编码不冲突，Bytes 只在结尾产生 `0x00 0x00`）。
- 二级 value = `pk_bytes`（回表免再解码 key）。
- 聚簇 value = `encode_row(row, schema)`（完整行）。

编码/解码函数放在 `ferrumdb-page/src/key.rs`（Value/ColumnType 属 page 域），engine 消费。需配套 decode（回表时从 key 取 pk、scan 时还原索引 key 边界）。

## 4. 数据流

- **create_table**：`Space::allocate_page()` 建空聚簇树 → `PersistentBtree::create` → catalog 登记。
- **create_index**：校验列下标 → 分配页建空二级树 → catalog 登记。
- **insert**（原子性顺序）：
  1. 聚簇 `get(pk)`：已存在 → `DuplicateKey`（主键冲突，engine.rs:86 语义）。
  2. 每个 `is_unique` 二级：`get(index_key)` → `Some` → `DuplicateKey`（先全部探测，后写入，保证无部分写入）。
  3. 写聚簇（root 分裂则回写 root）。
  4. 写各二级（复合 key → pk；root 分裂则回写 root）。
- **get_by_pk**：聚簇 `get(pk_bytes)` → `decode_row`。
- **get_by_index**：二级 `get(encode(index_key) ∥ terminator)`（前缀点查）→ 取 pk → 聚簇回表。非唯一索引返回首个（pk 最小）。
- **scan（聚簇）** / **scan_index（二级）**：`PersistentBtree::scan_range(start, end)` 物化为 `Vec<(key,value)>` → 二级每条回表 → 包成 `Box<dyn Iterator>`。RangeBound 上下界均为 `Value`，编码成保序字节。
- **持久化**：树随 Space 文件持久；catalog 内存态（重启后按记录的 root id 重开树验证，见 AC4）。

## 5. 契约与兼容性

- **trait 新增**（engine.rs，保持 object-safe，无 generic 方法）：
  - `create_index(&mut self, table: &str, meta: IndexMeta) -> Result<(), EngineError>`
  - `get_by_index(&self, table: &str, index: &str, key: Value) -> Result<Option<Row>, EngineError>`
  - `scan_index<'a>(&'a self, table: &str, index: &str, range: RangeBound) -> Result<RowIterator<'a>, EngineError>`
- 既有方法 `create_table/drop_table/insert/update/delete/get_by_pk/scan/begin/commit/rollback` 中，本阶段实现 `create_table/insert/get_by_pk/scan`；`update/delete` 返回 `Unsupported`（索引维护随阶段 7）。
- `StubEngine` 测试补 3 个新方法（`Unsupported`）；`ferrumdb-sql` 的 object-safe 冒烟测试继续通过。
- **无磁盘格式破坏**：新 key 编码仅用于 engine 写入路径；superblock 不改（阶段 3 的 `root_page_id` 临时字段本阶段不用）。
- `EngineError::DuplicateKey` 复用，不新增错误变体。

## 6. 关键权衡

| 决策 | 选择 | 理由 |
|------|------|------|
| scan 惰性 vs 物化 | 物化为 Vec | 简单、正确优先；plan.md 阶段 6 v1 亦如此。惰性迭代留后续 |
| Space 直连 vs BufferPool | Space 直连 | 阶段 6 聚焦正确性；BufferPool 接入属阶段 7 |
| catalog 持久化 | 内存态 | plan 阶段 7 明确"DDL 元数据持久化"；本阶段 AC4 只验存储层 |
| 唯一索引 | 本阶段强制检查 | 用户已确认；成本低（insert 多一次 get）且阶段 8 SQL 直接受益 |
| 复合键设计 | 统一 `index_key ∥ pk` | 一套结构同时支持唯一/非唯一，避免两套 |

## 7. 风险与回滚

- **key 编码边界冲突**（Bytes 0x00-escape 与分隔符）：严格单射 + 边界单测（负 i64、含 0x00 的 Bytes、跨列）。发现冲突只影响新编码模块，不影响既有行编码。
- **root split 后 root id 失步**：insert 后强制回写 catalog；用大量插入（≥ 触发多次根分裂）的测试覆盖。
- 本阶段全部为**新增代码 + trait 方法增加**，不修改既有行为；`cargo test` 基线（85 测试）应保持全绿。回滚 = 删除新增模块/方法，风险低。
