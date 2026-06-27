# FerrumDB 开发计划

本文档是 FerrumDB 的主路线图。每个模块给出：**目标、依赖、实现步骤、验收标准、常见坑**。
代码由开发者手写；完成一个模块后提交 Review。

---

## 总览

```mermaid
flowchart LR
    S0[阶段0 骨架]
    S1[阶段1 Page]
    S2[阶段2 B+Tree]
    S3[阶段3 持久化]
    S4[阶段4 Buffer Pool]
    S5[阶段5 Redo Log]
    S6[阶段6 二级索引]
    S7[阶段7 Engine]
    S8[阶段8 Server]
    S9[阶段9 事务]
    S10[阶段10 MVCC]

    S0 --> S1 --> S2 --> S3 --> S4 --> S5 --> S6 --> S7 --> S8 --> S9 --> S10
```

| 阶段 | 内容 | 参考周期 |
|------|------|----------|
| 0 | 项目骨架 | 1 周 |
| 1 | Page 基础 | 2 周 |
| 2 | 行编码 + 内存 B+Tree | 3 周 |
| 3 | 持久化 B+Tree + 表空间 | 3 周 |
| 4 | Buffer Pool | 2 周 |
| 5 | Redo Log | 3 周 |
| 6 | 二级索引 | 2 周 |
| 7 | StorageEngine 整合 | 2 周 |
| 8 | 极简 Server | 4 周 |
| 9 | 事务 | 4 周 |
| 10 | MVCC | 6 周 |

| 里程碑 | 能力 | 预计 |
|--------|------|------|
| **M1** | 单文件表空间 + 聚簇 B+Tree + Buffer Pool + Redo 恢复 | ~3 个月 |
| **M2** | 二级索引、范围扫描、`BEGIN/COMMIT` | +1~2 个月 |
| **M3** | 简化 MVCC、一种隔离级别 | 长期 |

**成功定义**：与 InnoDB **设计思想一致**（页、B+Tree、WAL、MVCC），不要求字节级兼容 `.ibd` 格式。

---

## 阶段 0 — 项目骨架

**Crate**：workspace 全部成员（空壳即可）

### 目标

- Cargo workspace 可 `cargo build` / `cargo test`
- 各 crate 职责边界清晰（见 [architecture.md](architecture.md)）

### 实现步骤

1. 确认根 `Cargo.toml` workspace members 与 [README](../README.md) 一致
2. 每个 crate 保留 `lib.rs` 占位与 crate 级文档注释
3. 在 `ferrumdb-engine` 中声明 `StorageEngine` trait（仅签名，无实现）

### 验收标准

- [x] `cargo build` 无错误
- [x] `cargo test` 通过（允许空测试）
- [x] `StorageEngine` trait 文档说明各方法语义

### StorageEngine trait 方法清单（后续逐步实现）

| 方法 | 阶段 |
|------|------|
| `create_table` | 7 |
| `insert` / `update` / `delete` | 7 |
| `get_by_pk` | 7 |
| `scan_range` | 6 |
| `begin` / `commit` / `rollback` | 9 |

---

## 阶段 1 — Page 基础

**Crate**：`ferrumdb-page`

### 目标

实现 16KB 固定大小页，带页头序列化/反序列化。

### 核心数据结构

**常量**

- `PAGE_SIZE = 16384`

**PageHeader（建议字段，可先实现子集）**

| 字段 | 类型 | 说明 |
|------|------|------|
| `page_id` | u32 | 页在表空间中的逻辑编号 |
| `page_type` | u8 | 数据页 / 索引页 / 空闲列表等 |
| `lsn` | u64 | 最后修改该页的 Log Sequence Number |
| `checksum` | u32 | 页完整性校验 |

**Page**

- 持有 `PageHeader` + 用户区字节（`PAGE_SIZE - header/footer`）

### 实现步骤

1. 定义 `PageType` 枚举（`Index`, `Data`, `Free` 等）
2. 实现 `Page::new(page_id, page_type)`
3. 实现 `Page::to_bytes() -> [u8; PAGE_SIZE]`
4. 实现 `Page::from_bytes(&[u8; PAGE_SIZE]) -> Result<Page>`
5. 实现 checksum：写入时计算并填入 header；读取时校验
6. 单元测试：round-trip、篡改检测

### 验收标准

- [x] 序列化长度恒为 16384
- [x] round-trip 后 header 字段一致
- [x] 修改任意字节后 `from_bytes` 返回 checksum 错误

### 常见坑

- 字节序：全项目统一 **little-endian**（与 x86/多数 Rust 默认一致），文档中写明
- 页头大小：预留 footer（如 8 字节），避免以后改布局破坏兼容性
- 不要在此阶段引入 async 或文件 I/O

### 建议测试

```text
test_page_round_trip
test_page_checksum_detects_corruption
test_page_invalid_length
```

---

## 阶段 2 — 行编码 + 内存 B+Tree

**Crate**：`ferrumdb-page`（行编码）、`ferrumdb-btree`（树）

### 目标

- 页内用 **Slotted Page** 布局存储变长行
- 内存 B+Tree：插入、查找、分裂、范围扫描

### 行编码（ferrumdb-page）

**步骤**

1. 定义 `Row` / `Value`（`Null`, `I64`, `Bytes`, …）
2. 定义 `Schema`（列名、类型、nullable、主键列）
3. 实现 `encode_row(row, schema) -> Vec<u8>`
4. 实现 `decode_row(bytes, schema) -> Row`
5. Slotted Page：slot directory 在页尾，记录 `(offset, len)`；插入时从空闲空间分配

**验收**

- [ ] 定长 + 变长 + NULL 字段 round-trip
- [ ] 单页满时返回 `PageFull` 错误（为分裂做准备）

### B+Tree（ferrumdb-btree，纯内存）

**步骤**

1. 定义 `BTreeKey`（有序，可比较）、`BTreeValue`（行或行 ID）
2. 内部节点：keys + 子节点指针
3. 叶子节点：keys + values，叶子间双向链表（范围扫描）
4. 实现 `insert`, `get`, `scan(from, to)`
5. 实现节点分裂：叶满 → 分裂 + 向上传播；根分裂 → 树增高

**验收**

- [ ] 插入 10_000 随机 key 后均可查找
- [ ] 范围扫描结果有序
- [ ] 分裂后树仍平衡（所有叶同层）

### 常见坑

- B+Tree 与 B-Tree：数据**只在叶子**；内部节点只存 key 作路由
- 分裂 key 选取：InnoDB 通常取中间；保持一致即可
- 先不做并发；单线程 `&mut self`

---

## 阶段 3 — 持久化 B+Tree + 表空间

**Crates**：`ferrumdb-btree`, `ferrumdb-space`

### 目标

B+Tree 节点映射到 `PageId`；表空间文件按页读写。

### 表空间（ferrumdb-space）

**步骤**

1. 文件布局：第 0 页 superblock（magic、版本、页大小、总页数）
2. `Space::allocate_page() -> PageId`
3. `Space::read_page(id) -> Page`
4. `Space::write_page(id, page)`
5. 空闲页管理：空闲链表或 bitmap（先选一种）

### 持久化 B+Tree

**步骤**

1. 节点序列化进 `Page` 用户区（自定义格式，文档化）
2. `PersistentBTree::get/insert` 通过 `Space` 加载/写回页
3. 根页 `PageId` 存在 superblock 或单独元数据

**验收**

- [ ] 进程重启后（重新 open 文件）数据仍在
- [ ] 插入 1000 条后 `get` 正确

### 常见坑

- 写页不是原子操作：阶段 5 用 Redo 保证崩溃一致性；此阶段可接受“崩溃可能损坏”（先写测试用例占位）
- `PageId` 与文件 offset：`offset = page_id * PAGE_SIZE`

---

## 阶段 4 — Buffer Pool

**Crate**：`ferrumdb-buffer`

### 目标

内存缓存热页，减少磁盘 I/O；管理 pin 与脏页。

### 核心概念

| 概念 | 说明 |
|------|------|
| **Frame** | 内存中一个 `Page` 的槽位 |
| **PageId → Frame** | 哈希映射 |
| **Pin** | 使用中的页不可 evict |
| **Dirty** | 已修改未刷盘 |

### 实现步骤

1. `BufferPool::new(pool_size_frames, space: Arc<Space>)`
2. `get_page(page_id) -> PageGuard`：命中则 pin；未命中从 disk 读入，LRU 淘汰 unpinned 页
3. `PageGuard` Drop 时 unpin；若 dirty 则标记
4. `flush_page` / `flush_all`：写回 `Space`
5. 修改 B+Tree 路径：经 BufferPool 而非直接 `Space`

### 验收标准

- [ ] 同一 `PageId` 多次 `get_page` 返回一致内容
- [ ] pool 小于总页数时 LRU 淘汰生效（可用 spy/mock 计数 disk read）
- [ ] flush 后重启可读

### 常见坑

- 淘汰脏页前必须先 flush
- 避免死锁：持锁顺序固定（如先 pool 锁再 page 锁）
- `PageGuard` 用 RAII 表达 pin 生命周期

---

## 阶段 5 — Redo Log

**Crate**：`ferrumdb-wal`

### 目标

WAL：先写 log 再改页；崩溃后 redo 恢复已提交修改。

### 实现步骤

1. Redo 文件 append-only；每条 record：`lsn, page_id, offset, payload`
2. 全局 `next_lsn` 单调递增
3. 修改页时：先 append redo，再改内存页并更新页 `lsn`
4. `checkpoint`：记录已刷盘的最大 lsn（简化版）
5. 启动 `recover()`：从 checkpoint 后 replay redo 到页

### 验收标准

- [ ] 随机时刻 kill 进程（测试里模拟），重启后已 insert 的数据不丢
- [ ] 未 flush 的页仅靠 redo 可恢复

### 常见坑

- Redo 是**物理日志**（页片段），不是 SQL 语句
- `fsync` 策略：提交路径上至少 log fsync（可先同步写，后优化 batch）
- 与 Buffer Pool 协作：recovery 时可能 bypass pool 直接写 Space

---

## 阶段 6 — 二级索引

**Crates**：`ferrumdb-btree`, `ferrumdb-engine`

### 目标

非主键索引：叶子存 `(index_key, primary_key)`；查询时回表。

### 实现步骤

1. `IndexMeta`：列集合、是否唯一
2. 每张表：一个聚簇 B+Tree + N 个二级 B+Tree
3. `insert`：先写聚簇，再写所有二级索引
4. `get_by_pk`：只走聚簇
5. `get_by_index`：二级 → 拿到 pk → 回表
6. `scan_range`：聚簇或二级上的范围扫描

### 验收标准

- [ ] 二级索引点查正确
- [ ] 范围扫描 + 回表结果正确
- [ ] 同一 pk 多二级索引一致

---

## 阶段 7 — StorageEngine 整合

**Crate**：`ferrumdb-engine`

### 目标

对外统一 API，串联 space / buffer / wal / btree。

### 实现步骤

1. 实现阶段 0 定义的 `StorageEngine` trait
2. `TableCatalog`：表名 → schema + 根页 ids
3. `create_table`：分配元数据页与空 B+Tree
4. DDL 元数据持久化（可先 JSON 放 superblock 扩展区，后改专用 catalog 页）

### 验收标准

- [ ] 集成测试：create → insert × N → get_by_pk
- [ ] 集成测试：崩溃恢复后数据完整

---

## 阶段 8 — 极简 Server

**Crates**：`ferrumdb-protocol`, `ferrumdb-sql`, `ferrumdb-server`

### 目标

TCP 服务，支持最小 MySQL 协议子集与 SQL。

### 支持 SQL（第一版）

```sql
CREATE TABLE t (id INT PRIMARY KEY, name VARCHAR(255));
INSERT INTO t VALUES (1, 'hello');
SELECT * FROM t WHERE id = 1;
```

### 实现步骤

1. **protocol**：握手、OK/Error 包、简单 query 响应
2. **sql**：手写递归下降 parser（或先用 `sqlparser` crate 评估）
3. **server**：`tokio` accept 连接 → 读包 → 解析 → 调 `StorageEngine` → 写结果集

### 验收标准

- [ ] `mysql` CLI 或 `nc` 能连上并执行上述 SQL
- [ ] 错误 SQL 返回 Error 包

### 常见坑

- 字符集先固定 `utf8mb4` 或 `latin1`，文档说明
- 不必实现 prepared statement（后期）

---

## 阶段 9 — 事务

**Crate**：`ferrumdb-txn`

### 目标

`BEGIN / COMMIT / ROLLBACK`；Undo 支持回滚。

### 实现步骤

1. 事务 ID 分配
2. Undo log：update/delete 前写旧版本
3. Commit：redo fsync 后标记事务完成
4. Rollback：沿 undo 链恢复

### 验收标准

- [ ] 事务内 insert 后 rollback 不可见
- [ ] 事务 commit 后其他会话可见（单进程多连接模拟）

---

## 阶段 10 — MVCC

**Crate**：`ferrumdb-txn`

### 目标

非锁定一致性读；Read View；一种隔离级别（建议先 **Read Committed** 或简化 **RR**）。

### 实现步骤

1. 每行增加 `trx_id`, `roll_ptr`（指向 undo）
2. `ReadView`：活跃事务列表、`min_trx_id`、`max_trx_id`
3. 读时沿版本链找可见版本
4. 写时用行锁或乐观锁（先简单 mutex）

### 验收标准

- [ ] 快照读不阻塞写（在单引擎测试场景）
- [ ] 写偏斜等经典用例有测试文档

---

## 学习资源

| 资源 | 用途 |
|------|------|
| [MySQL Internals Manual](https://dev.mysql.com/doc/internals/en/) | Server 与插件接口 |
| 《MySQL 技术内幕：InnoDB 存储引擎》 | 页、索引、事务 |
| [tikv/tikv](https://github.com/tikv/tikv) | Rust MVCC/Raft 参考 |
| [spacejam/sled](https://github.com/spacejam/sled) | 嵌入式 B-tree/WAL |

---

## Review 检查清单

每阶段 Review 时对照：

- [ ] 是否满足该阶段验收标准
- [ ] 错误类型是否用 `thiserror` 表达清晰
- [ ] 公开 API 是否有 doc comment
- [ ] 单元/集成测试是否覆盖 happy path + 关键 failure
- [ ] 是否与下一阶段接口对齐（见 architecture.md）

---

## 变更记录

| 日期 | 说明 |
|------|------|
| 2025-06-27 | 初版计划，项目命名 FerrumDB |
