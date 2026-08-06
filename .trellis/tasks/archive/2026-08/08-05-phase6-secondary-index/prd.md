# 阶段 6：二级索引与回表

## Goal

实现非主键二级索引：IndexMeta、每表聚簇 B+Tree + N 个二级 B+Tree、insert 同步维护、get_by_index 回表、scan_range 支持。交付"最小可回表引擎"，端到端可验收（点查 + 范围扫描 + 回表正确）。

## Background / Confirmed Facts

- 项目已完成阶段 0-5：Page、行编码、内存/持久化 B+Tree、Space、BufferPool、WAL 均就绪，`cargo test` 全绿。
- `PersistentBtree`（crates/ferrumdb-btree/src/persistent.rs）以 `(Vec<u8> key, Vec<u8> value)` 存储，支持 `create/open/get/insert/scan_range`；**无 delete**；重复 key 覆盖。
- 二级索引叶子存 `(index_key, primary_key)`：用复合键 `index_key ∥ pk` 编码，非唯一索引在树内也保持 key 唯一。
- **架构约束**：`ferrumdb-btree` 不得依赖 `ferrumdb-buffer` / `ferrumdb-wal` / `ferrumdb-space`（见 .trellis/spec/ferrumdb-btree/backend/index.md:21-23）；跨层接线属于 `ferrumdb-engine`。
- `StorageEngine` trait（crates/ferrumdb-engine/src/engine.rs）目前为纯定义（阶段 0）；`FerrumEngine` 实现计划在阶段 7。阶段 6 需实现一个**最小 engine**（内存 TableCatalog + 复用现有 PersistentBtree/PageSource）以端到端验收回表。
- engine 的 `update` / `delete` 本身尚未实现 → 本阶段二级索引维护只覆盖 **insert** 路径；update/delete 的索引维护随阶段 7 一并完成（engine 层 update/delete 落地时）。

## Requirements

- R1 `IndexMeta`：列集合、是否唯一。
- R2 每张表：1 个聚簇 B+Tree + N 个二级 B+Tree。
- R3 `insert`：先写聚簇，再同步写所有二级索引（二级 key = `index_key ∥ pk`）。
- R4 `get_by_pk`：只走聚簇。
- R5 `get_by_index`：二级 → 取 pk → 回表取整行。
- R6 范围扫描：trait 上 `scan` 走聚簇、`scan_index` 走二级索引；二级扫描内部完成回表。
- R7 二级索引复用 `PersistentBtree` / PageSource，持久化（随阶段 3 基础设施自然获得）。
- R8 `StorageEngine` trait 保持 object-safe；SQL 层的 object-safe 冒烟测试（crates/ferrumdb-sql/src/lib.rs）继续通过。
- R9 唯一索引强制检查：insert 前对每个唯一二级索引探测；冲突返回 `EngineError::DuplicateKey`，且**不写入任何索引**（聚簇不写、所有二级不写，保持原子性）。

## Acceptance Criteria

- [ ] AC1 二级索引点查正确：`get_by_index` 对已插入行返回完整行；不存在的 key 返回 `None`。
- [ ] AC2 范围扫描 + 回表正确：二级索引上 `scan_range` 结果按索引 key 有序，且回表得到正确完整行。
- [ ] AC3 同一 pk 多个二级索引数据一致：insert 后所有二级索引均能查到该 pk。
- [ ] AC4 持久化（存储层）：Space 文件层持久；用记录的 root page id 重新打开聚簇/二级 `PersistentBtree` 后，点查/扫描数据正确。完整 catalog 重启恢复归阶段 7。
- [ ] AC5 `cargo build` / `cargo test` 全绿；新增单元/集成测试覆盖 R1-R9 的 happy path。
- [ ] AC6 唯一索引冲突：插入已存在的唯一 key 返回 `EngineError::DuplicateKey`；冲突后数据无部分写入。
- [ ] AC7 非唯一索引：多个不同 pk 可共享同一索引 key，`scan_range` 按 `(index_key, pk)` 有序返回。

## Out of Scope

- DDL 元数据持久化（TableCatalog 持久化、create_table 跨重启）→ 阶段 7。
- `update` / `delete` 及其二级索引维护 → 阶段 7。
- `begin` / `commit` / `rollback` 事务 → 阶段 9。
- WAL 接入 engine 写入路径 → 阶段 7。
- btree 层 delete 操作（当前 B+Tree 无 delete）→ 阶段 7 需要时再补。

## Open Questions

（无阻塞问题）

## Key Decisions

- KD1 二级索引 key 统一用复合编码 `index_key ∥ pk`（设计上同时支持唯一/非唯一，避免两套结构）。
- KD2 阶段 6 引入最小 `FerrumEngine`（内存 catalog + PageSource 持久树），仅实现本阶段所需 trait 方法子集（create_table/create_index/insert/get_by_pk/get_by_index/scan/scan_index），其余方法继续返回 `Unsupported`。
- KD3 二级索引复用 `PersistentBtree` 与 PageSource（Space/BufferPool），不新建存储后端。
- KD4 唯一索引强制检查在本阶段实现（用户已确认）：insert 对唯一索引先探测，冲突即 `DuplicateKey` 且整体不写入。
- KD5 `get_by_index` 作为 `StorageEngine` trait 新增方法（当前 trait 无此方法），保持 object-safe。
- KD6 catalog 为内存态；DDL 元数据持久化归阶段 7（superblock.rs:33 注释与 plan 阶段 7 明确）。
