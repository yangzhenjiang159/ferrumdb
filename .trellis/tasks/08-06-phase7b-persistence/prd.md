# 阶段 7b：catalog 持久化与 WAL 崩溃恢复

## Goal

让 `FerrumEngine` **跨重启可用**：表结构（catalog）落盘 + 写路径经 WAL + 启动时崩溃恢复。父任务 `phase7-engine` 的子任务，与 7a（DML）无依赖。

## Background / Confirmed Facts

- 阶段 6 catalog 为内存态 `HashMap<String, TableMeta>`，不落盘；`FerrumEngine` 持有 `RefCell<Space>`。
- Superblock（`space/src/superblock.rs`）：user_data 16344 字节，固定字段占用约 30 字节（magic/version/page_size/free_head/root/last_lsn），剩余 ~16314 字节可作扩展区。
- WAL（`ferrumdb-wal`）已就绪：`append(page_id, offset, payload)` / `recover(FnMut(&RedoRecord))` / `checkpoint(max_flushed_lsn)`；record 为物理 redo（lsn/page_id/offset/payload）。整页写入可用 `(page_id, 0, 整页)`。
- `PersistentBtree::open(source, root_page_id)` 可在已知 root 时重开树；catalog 需要持久化「表名 → schema + 各树 root id」才能在重启后重开所有树。
- 阶段 6 AC4 已验存储层持久（Space 文件层）；本阶段补 **catalog 元数据** 持久化 + **WAL 崩溃恢复**。
- `docs/plan.md` 阶段 7 提示：catalog 持久化「可先 JSON 放 superblock 扩展区，后改专用 catalog 页」。

## Requirements

- R1 **catalog 持久化**：将表集合（每表：name、schema、clustered_root、各二级索引 meta+root）序列化写入 superblock 扩展区（JSON，v1）。
- R2 **启动恢复 catalog**：`FerrumEngine::open_or_create` 打开已有文件时，从 superblock 读回 catalog，用记录的各 root id `PersistentBtree::open` 重开树；`get_by_pk`/`get_by_index`/`scan` 立即可用。
- R3 **写路径经 WAL**：insert/update/delete（含建树、根分裂产生的新页）写页前先 `wal.append` 整页 redo；engine 持有 WAL。
- R4 **崩溃恢复**：启动时 `wal.recover` replay 未 checkpoint 的记录到 Space 页；用 `checkpoint(lsn)` 记录已刷盘最大 lsn 以缩短 replay。
- R5 `create_table`/`create_index` 后立即持久化 catalog（含新 root id）；`drop_table` 后移除。
- R6 superblock 的 `root_page_id` 临时字段不再承担 catalog 职责（保留兼容读取）；catalog 走扩展区。
- R7 `StorageEngine` trait 保持 object-safe；`ferrumdb-sql` 冒烟测试继续通过。

## Acceptance Criteria

- [ ] AC1 **跨重启**：create table + create index + insert × N → 关闭并重开 engine（同一文件）→ `get_by_pk`/`get_by_index`/`scan` 数据与 schema 完整。
- [ ] AC2 **崩溃恢复**：模拟「写页未 fsync + WAL 已 fsync」中断（测试构造）→ 重启后 WAL replay，已提交 insert 的数据不丢。
- [ ] AC3 catalog 变更（create/drop table）重启后仍生效：drop 的表重启后不存在，新建的表重启后存在。
- [ ] AC4 根分裂后的新 root id 持久化：大量 insert 触发分裂 → 重启后仍能按新 root 重开树、数据正确。
- [ ] AC5 `cargo build` / `cargo test` 全绿；`cargo clippy --all-targets -- -D warnings` 干净；`ferrumdb-sql` object-safe 测试通过。

## Out of Scope

- DML 方法（update/delete/drop_table）→ 子任务 7a。
- 事务、MVCC → 阶段 9/10。
- BufferPool 接入（用户确认拆 2 子任务）。
- catalog 专用页（多页 catalog 结构）→ 后续阶段；本阶段 JSON 放 superblock 扩展区。
- WAL 的 batch/group commit、后台刷盘优化。

## Open Questions

（无阻塞问题）

## Key Decisions

- KD1 catalog 格式：JSON 放 superblock 扩展区（plan 阶段 7 明示优先方案）；引入独立 catalog 页留后续。
- KD2 WAL 记录粒度：整页 redo `(page_id, 0, page_bytes)`——简单、与 btree 现有 `write_page` 对齐；不做页内 diff。
- KD3 WAL 接入层：engine 持有 `Wal`，写路径通过「先 append 再 write_page」包装；不改 btree（spec 约束 btree 不依赖 wal）。
- KD4 恢复顺序：启动时先 read superblock 得 catalog（表 + root ids）→ `wal.recover` replay 到 Space → 按 root id 重开树。replay 只需覆盖数据页，catalog 在 superblock 独立于 WAL。
- KD5 `create_table`/`create_index`/`drop_table` 为「先改内存 + 写 catalog 到 superblock」，WAL 只负责数据页；catalog 变更即时落 superblock（fsync）。
