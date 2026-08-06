# 阶段 7a：DML 补全（update/delete/drop_table）

## Goal

实现 `StorageEngine` 剩余的写路径方法：`update`、`delete`、`drop_table`，并保证与已有聚簇/二级索引一致。父任务 `phase7-engine` 的子任务，只做 DML，不做持久化/WAL（见父任务 prd 任务树）。

## Background / Confirmed Facts

- 阶段 6 `FerrumEngine` 已有 `create_table`/`create_index`/`insert`/`get_by_pk`/`get_by_index`/`scan`/`scan_index`；`insert` 会同步维护所有二级索引（复合 key `index_key ∥ pk`，value = pk_bytes）。
- catalog 持有 `PersistentBtree` 句柄（`catalog.rs`），root id 随分裂自动更新，无需回写。
- `PersistentBtree::delete`（`persistent.rs:309`）为 v1：**下探到叶子后只检查该叶子**，不做叶子链表遍历、不做下溢 rebalance。`get`（:95）会沿叶子链表继续找——两者语义不一致，**可能漏删**（key 恰好在更靠后的叶子时）。
- `Space::free_page(page_id)` 可将页归还空闲链表（`space.rs`）。btree 未暴露遍历全部节点页的 API。
- trait 签名：`update(&mut self, table, pk: Value, row: Row) -> Result<(), EngineError>`；`delete(&mut self, table, pk: Value) -> Result<(), EngineError>`；`drop_table(&mut self, name) -> Result<(), EngineError>`。

## Requirements

- R1 `update(table, pk, row)`：定位 pk 所在行，替换为 `row`；**主键列不可变**（`row` 中 pk 列必须等于 `pk`，否则 `Internal`）。
- R2 `update` 后二级索引一致：对每个二级索引，若索引列值变化 → 删旧索引项 + 插新索引项；若未变 → 不动。
- R3 `delete(table, pk)`：删除聚簇行 + 所有二级索引项；幂等（行不存在返回 `Ok(())`）。
- R4 `drop_table(name)`：移除 catalog 条目 + 释放聚簇/全部二级树的节点页（`Space::free_page`）。
- R5 修复 `PersistentBtree::delete` 漏删：删除前需定位到 key 实际所在的叶子（沿叶子链表），或至少与 `get` 行为一致。
- R6 `update`/`delete`/`drop_table` 对不存在表的返回 `TableNotFound`；`update` 对不存在行返回 `EngineError::RowNotFound`（新增变体）。
- R7 trait 保持 object-safe；`ferrumdb-sql` 冒烟测试继续通过。

## Acceptance Criteria

- [ ] AC1 `update` 后 `get_by_pk` 返回新行；索引列未变时二级索引仍可查；索引列改变时旧索引键失效、新索引键可查。
- [ ] AC2 `update` 对不存在的行返回 `RowNotFound`；对不存在的表返回 `TableNotFound`。
- [ ] AC3 `delete` 后 `get_by_pk` 返回 `None`，所有二级索引均查不到该 pk；`delete` 不存在的行返回 `Ok(())`。
- [ ] AC4 `drop_table` 后 `get_by_pk` 返回 `TableNotFound`；表占用的页被释放（重新 `allocate_page` 复用同一页 id）。
- [ ] AC5 btree `delete` 在高分裂场景（> ORDER 键数、触发多层分裂）不漏删、不误删。
- [ ] AC6 `cargo build` / `cargo test` 全绿；`cargo clippy --all-targets -- -D warnings` 干净；`ferrumdb-sql` object-safe 测试通过。

## Out of Scope

- catalog 持久化 / WAL 崩溃恢复 → 子任务 7b。
- 事务、MVCC → 阶段 9/10。
- btree delete 的下溢 rebalance/merge（v2）——只保证删除正确，不做节点合并。
- `drop_table` 之外的空间碎片整理。

## Open Questions

（无阻塞问题）

## Key Decisions

- KD1 `delete` 幂等（行不存在 → `Ok(())`），与 MySQL DELETE 0 rows 一致；`update` 不存在行 → `RowNotFound`（新增变体，spec 要求 known case 用特定变体）。
- KD2 主键列不可变（`row` 中 pk 列须等于 `pk`），避免「改主键 = 删 + 插」的复杂路径（留后续）。
- KD3 btree `delete` 修复最小化：对齐 `get` 的叶子链表遍历；不做 rebalance。
- KD4 `drop_table` 页释放需要 btree 暴露「遍历所有节点页」能力（engine 才能 `free_page`）；否则退化为不释放页（catalog 移除即可），AC4 相应降级。
