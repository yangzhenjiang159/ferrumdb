# 阶段 7：StorageEngine 整合

## Goal

完成 `StorageEngine` trait 的剩余实现，打通「DML 补全 + DDL 元数据持久化 + 崩溃恢复」，使 engine 成为跨重启可用的完整存储层。阶段 8（极简 Server）将直接消费此 API。

## Background / Confirmed Facts

- 阶段 6 已交付最小 `FerrumEngine`（`ferrum_engine.rs`）：实现 `create_table`/`create_index`/`insert`/`get_by_pk`/`get_by_index`/`scan`/`scan_index`，110 测试全绿。
- 阶段 6 明确移交到阶段 7 的三项：`update`/`delete` 及二级索引维护、DDL 元数据持久化（catalog 跨重启）、WAL 接入 engine 写入路径。
- 当前 `FerrumEngine` 中 `update`/`delete`/`drop_table` 返回 `Unsupported`；`begin`/`commit`/`rollback` 返回 `Unsupported`（阶段 9）。
- catalog 为内存态 `HashMap<String, TableMeta>`（`catalog.rs`），表结构 + 树句柄不落盘。
- `PersistentBtree`（`persistent.rs`）已有 `delete`（v1：删叶子条目，**不做下溢 rebalance**），并有 `insert`/`get`/`scan_range`。
- WAL（`ferrumdb-wal`）已就绪：`append(page_id, offset, payload)` / `recover(callback)` / `checkpoint(lsn)`，物理 redo 记录 `(lsn, page_id, offset, payload)`。
- `BufferPool` + `BufferPoolSource`（`buffer/src/source.rs`）实现 `PageSource`，可包一层 Space 作为缓存；当前 engine 直接持有 `Space`。
- Superblock（`space/src/superblock.rs`）有 `root_page_id: Option<u32>` 与 `last_lsn: u64`，无 catalog 序列化字段；superblock 用户区 16344 字节，固定字段约占 30 字节，剩余可作为扩展区。
- `docs/plan.md` 阶段 7 验收：`create → insert × N → get_by_pk`；**崩溃恢复后数据完整**（要求 WAL 接入）。
- `docs/plan.md` 阶段 7 提示：catalog 持久化「可先 JSON 放 superblock 扩展区，后改专用 catalog 页」。

## Requirements

- R1 `update`：按 pk 更新行；更新后**二级索引保持一致**（索引列变化时删旧索引项 + 插新索引项）；pk 不存在返回错误。
- R2 `delete`：按 pk 删除行，并同步删除**所有二级索引项**；幂等（删除不存在的行返回 `Ok(())`，与 MySQL DELETE 0 rows 语义一致）。
- R3 `drop_table`：删除 catalog 条目并释放表占用的页（聚簇 + 全部二级树页）。
- R4 **catalog 持久化**：表结构（schema、根页 id）跨重启恢复；重启后 `get_by_pk`/`get_by_index`/`scan` 数据正确。
- R5 **WAL 接入 engine 写入路径**：insert/update/delete 写页前先 append redo；启动时 replay WAL 恢复未刷盘页。
- R6 BufferPool 接入（若在范围内）：engine 经 `BufferPoolSource` 而非直连 `Space`。
- R7 `StorageEngine` trait 保持 object-safe；`ferrumdb-sql` 冒烟测试继续通过。
- R8 错误语义：`update` 对不存在的行返回 `EngineError::RowNotFound`（新增变体，比 `Internal` 更精确，符合 spec「known case 用特定变体」）；`delete` 幂等返回 `Ok(())`。`drop_table` 对不存在的表返回 `TableNotFound`。

## Acceptance Criteria

**7a（DML 补全）**
- [ ] AC1 `update` 后 `get_by_pk` 返回新行；索引列未变时二级索引仍可查到该 pk；索引列改变时旧索引键失效、新索引键可查。
- [ ] AC2 `delete` 后 `get_by_pk` 返回 `None`，所有二级索引均查不到该 pk；`update` 不存在行 → `RowNotFound`。
- [ ] AC3 `drop_table` 后 `get_by_pk` 返回 `TableNotFound`，表占用的页被释放（重新 allocate 复用）。

**7b（catalog 持久化 + WAL 崩溃恢复）**
- [ ] AC4 **跨重启**：create table + create index + insert × N → 关闭并重开 engine（同一文件）→ `get_by_pk`/`get_by_index`/`scan` 数据与 schema 完整。
- [ ] AC5 **崩溃恢复**：模拟未 fsync 即中断（或测试直接构造未落盘场景）→ 重启后 WAL replay，已提交 insert 的数据不丢。

**父任务整合门**
- [ ] AC6 `cargo build` / `cargo test` 全绿；`cargo clippy --all-targets -- -D warnings` 干净。
- [ ] AC7 `ferrumdb-sql` object-safe 冒烟测试继续通过。

## Out of Scope

- 事务 `begin`/`commit`/`rollback`、MVCC → 阶段 9/10。
- B+Tree 下溢 rebalance/merge（v2）——本阶段若 btree delete 的 v1 语义暴露正确性问题，只修「删除后查询/扫描正确」所需的最小部分。
- 二级索引的 `update`/`delete` 之外的索引类型（全文、hash 等）。
- 多表空间 / 每表独立文件（仍单文件）。
- **BufferPool 接入**：用户确认阶段 7 拆 2 子任务（DML + 持久化），BufferPool 不在其中，推迟到后续阶段。

## 任务树

父任务 `phase7-engine` 不直接实现，负责跨子任务验收与整合（R7/R8、AC6/AC7）。

| 子任务 | 负责 | 独立验收 |
|--------|------|----------|
| `08-06-phase7a-dml` | R1 update / R2 delete / R3 drop_table + 二级索引维护 | AC1/AC2/AC3 |
| `08-06-phase7b-persistence` | R4 catalog 持久化 / R5 WAL 崩溃恢复 | AC4/AC5 |

- 7a 不依赖 7b（纯内存路径即可验收）。
- 7b 的 WAL 恢复需覆盖 insert（7a 前已就绪）；update/delete 的恢复在 7b 中顺带覆盖（若 7a 已合入）。

## Open Questions

（无阻塞问题）

## Key Decisions

- KD1 范围划分（用户已确认）：父任务拆 2 子任务（7a DML 补全 / 7b 持久化与恢复）。
- KD2 catalog 持久化格式：**JSON 放 superblock 扩展区**（7b design 定稿；plan 阶段 7 明示优先方案；专用 catalog 页留后续）。
- KD3 BufferPool：**不接入**（用户确认拆 2 子任务，推迟后续阶段）。
- KD4 WAL 接入方式：engine 持 `WalPageSource`（包 Space + Wal）作为 `PageSource`，写页前 append 整页 redo（7b design 定稿）。
