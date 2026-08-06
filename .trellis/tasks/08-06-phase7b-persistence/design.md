# 阶段 7b：catalog 持久化与 WAL 崩溃恢复 — 技术设计

## 1. 架构与边界

```
FerrumEngine
  ├─ space: RefCell<Space>          // 页存储
  ├─ wal: Option<Wal>               // redo log（本阶段引入）
  ├─ catalog: TableCatalog           // 内存态（阶段 6）
  └─ 启动流程：
       open_or_create(path):
         Space::open/create
         Wal::open/create (同目录 <name>.wal)
         superblock 读 catalog → 重建 TableCatalog（表 + root ids）
         wal.recover(replay 到 Space)
         catalog 内每棵 PersistentBtree::open(root_id)
```

- 数据流：写路径（insert/update/delete）先 `wal.append(整页)` 再 `space.write_page`。
- catalog 序列化在 `superblock.rs` 的扩展区（独立于 WAL；DDL 变更即时落盘）。

## 2. Superblock 扩展区（catalog JSON）

### 布局

`superblock.rs` 现固定字段占 user_data[0..~30]，剩余 user_data[30..16344] 空闲。
新增 catalog 区域：

```
+------------------+  offset 0
| 现有固定字段(~30B)|  (magic/version/page_size/free_head/root/last_lsn)
+------------------+  offset 30
| CATALOG_MAGIC u32|  0xCA7A_1001，catalog 存在标记
| catalog_len  u32|  JSON 字节长度
| JSON bytes      |  serde_json 序列化的 catalog
+------------------+  offset 16344
```

- 偏移量定为常量（如 `OFF_CATALOG_MAGIC = 30`）。`SUPERBLOCK_BYTES` 相应扩展。
- 长度上限：`16344 - 30 - 8 = 16306` 字节，v1 足够（单文件少量表）。超限返回 `SpaceError::SuperblockTruncated`（新变体或复用）。

### 序列化格式（serde_json，v1）

```rust
#[derive(Serialize, Deserialize)]
struct CatalogSnapshot {
    version: u32,
    tables: Vec<TableSnapshot>,
}
#[derive(Serialize, Deserialize)]
struct TableSnapshot {
    name: String,
    schema: Schema,                 // 需要 Schema 实现 Serialize/Deserialize
    clustered_root: u32,
    indexes: Vec<IndexSnapshot>,    // { name, columns, is_unique, root }
}
```

- `Schema`/`ColumnType`/`Value` 在 `ferrumdb-page` 需 `serde` 支持（加 `#[derive(Serialize, Deserialize)]`，serde 作为 page 的可选/直接依赖）。
- 新增 `ferrumdb-space` 依赖 `serde` + `serde_json`（catalog 序列化归属 space，因 superblock 在 space）。

### 读写 API（superblock.rs）

```rust
impl Superblock {
    pub fn catalog_bytes(&self) -> Option<&[u8]>;         // 读扩展区（magic 校验）
    pub fn set_catalog_bytes(&mut self, bytes: &[u8]);    // 写扩展区（超长报错）
}
```
- `Space::write_superblock()` 已存在，DDL 变更后调用即 fsync。
- 无 catalog 的旧文件（阶段 6 产物）→ `catalog_bytes` 返回 None → engine 以空 catalog 启动（兼容）。

## 3. WAL 接入（KD2/KD3）

### 引擎持有 Wal

- `Wal::create(path)` / `Wal::open(path)`：与 tablespace 同目录、同前缀（如 `test.ibd.wal`）。
- `FerrumEngine` 字段 `wal: Option<Wal>`（`create_table` 前可能未初始化；统一在 `open_or_create` 初始化）。

### 写路径包装

```rust
// engine 内部 helper：先写 WAL 再写页
fn write_page_wal(&mut self, page_id: u32, page: &Page) -> Result<(), EngineError> {
    let payload = page.to_bytes();
    self.wal.append(page_id, 0, &payload)?;   // 整页 redo
    self.space.borrow_mut().write_page(page_id, page)?;
    Ok(())
}
```
- 但 btree 直接调 `source.write_page`，engine 无法拦截。方案：
  - **A. 改 btree 的 PageSource 为 WAL 包装**：新增 `WalPageSource<'a>` 实现 `PageSource`，内部包 `&mut Space` + `&mut Wal`，`write_page` 先 append。engine 把 btree 的 source 传 `WalPageSource`。**不改 btree 代码**（spec 约束成立），改动集中在 engine 构造 source。
  - B. 改 btree 支持回调——违反「btree 不依赖 wal」，排除。
- 选 **A**：`WalPageSource` 放 `ferrumdb-engine`（跨层接线属 engine，符合 phase 6 design 的边界声明）。

### WalPageSource

```rust
// ferrumdb-engine/src/wal_source.rs
pub struct WalPageSource<'a> {
    space: &'a mut Space,
    wal: &'a mut Wal,
}
impl PageSource for WalPageSource<'_> {
    fn read_page(&mut self, id) { self.space.read_page(id) }
    fn write_page(&mut self, id, page) {
        self.wal.append(id, 0, &page.to_bytes())?;
        self.space.write_page(id, page)
    }
    fn allocate_page(&mut self) { self.space.allocate_page() }
}
```
- 所有 btree 操作（insert/delete/scan/get/分裂）经 `WalPageSource` 后自动获得 WAL 日志。
- **扫描/读取不产生 WAL**（read 路径直连 space）。

### 崩溃恢复

```rust
// open_or_create 启动时：
let wal = Wal::open(&wal_path)?;
wal.recover(|rec| {
    // 整页 redo：offset 恒 0，payload 为整页
    let page = Page::from_bytes(&rec.payload)?;   // 或直接写 rec.payload 到 offset
    space.write_page(rec.page_id, page)?;
    Ok(())
})?;
```
- 只 replay `lsn > checkpoint_lsn` 的记录（WAL `recover` 已按 checkpoint 过滤，见 wal.rs:215）。
- replay 后，页即最新；内存 catalog 从 superblock 读（独立，无 WAL 依赖）。
- `checkpoint`：engine 可周期性调用（本阶段简化：每次 `create_table` 等 DDL 或固定时机）——记录已刷盘最大 lsn，缩短下次 replay。

## 4. 根分裂与 root id 持久化（KD5 / AC4）

关键顺序问题：insert 触发根分裂 → 新 root 页写入（经 WAL）→ **catalog 中的 root id 必须更新**。

- 内存：catalog 持有 `PersistentBtree` 句柄，分裂后 `tree.root_page_id()` 自动更新（阶段 6 已处理）。
- 持久：插入后若 root 变化，需把新 root 写回 superblock catalog。
  - 方案：`insert` 返回后，engine 对比操作前后的 `root_page_id`，变化则 `persist_catalog()`。
  - 或更简单：**每次写路径后都 `persist_catalog()`**（catalog 小，fsync 代价可接受；阶段 7 不做性能优化）。选后者，简单正确。

## 5. 数据流

- **open_or_create**：Space open/create → Wal open/create → superblock 读 catalog（旧文件无则空）→ `wal.recover` replay → 按 root id 重开树。
- **create_table**：建聚簇树 → 内存 catalog 登记 → `persist_catalog()`（写 superblock JSON）。
- **create_index**：同上。
- **drop_table**（7a 已实现）→ `persist_catalog()`。
- **insert/update/delete**：经 `WalPageSource` 写页 → 操作后 `persist_catalog()`（保 root 变化）。
- **崩溃窗口**：insert 的 WAL append + 页写是「append 先行」；若页写失败，WAL 有记录但页没写 → 重启 replay 补齐（恰好恢复）。若 catalog 未持久化新 root 但 WAL 有新 root 页 → 重启按旧 root 打开，找不到新 root 页 → **问题**。缓解：insert 前先确保 catalog root 已持久（先 persist 再 insert），或 root 变化时先 persist 再返回成功。**设计决定：insert 返回前若 root 变化，先 persist_catalog() 再返回。**

## 6. 兼容性与契约

- 旧文件（无 catalog 扩展区）→ 空 catalog 启动；阶段 6 的数据文件直接打开为空库（无表）。可接受（阶段 6 无持久 catalog 契约）。
- superblock `root_page_id` 字段保留，不再由 engine 使用（兼容读取，不写入新语义）。
- `Schema` 加 `Serialize/Deserialize`：不破坏现有 API，仅新增 derive。
- `#![deny(missing_docs)]`：新 pub 项（`WalPageSource`、catalog 序列化类型）需 doc。

## 7. 风险与回滚

| 风险 | 应对 |
|------|------|
| JSON catalog 超 superblock 容量 | 超限报错；v1 限制表数量；后续换专用 catalog 页 |
| root id 失步导致重启打不开树 | insert 返回前若 root 变则先 persist_catalog；AC4 高分裂测试覆盖 |
| WAL 整页 redo 体积大 | 阶段 7 接受；后续做页内 diff |
| replay 覆盖 catalog 页（page 0） | `WalPageSource` 不写 page 0（catalog 不走 WAL）；recover 跳过 page 0 |
| 旧文件兼容 | 无 catalog 区域 → 空库；不加迁移逻辑 |

回滚：新增模块（wal_source.rs、catalog 序列化）删除即可；`Schema` serde derive 可保留（无害）。
