# 阶段 7b：catalog 持久化与 WAL 崩溃恢复 — 执行计划

## 基线

- `cargo build` / `cargo test` 全绿（当前 110 测试，7a 合入后可能更多）；clippy 干净。
- 已读：space superblock.rs / wal wal.rs + record.rs / engine ferrum_engine.rs + catalog.rs / page row.rs（Schema）。
- 7b 依赖 7a 提供 `drop_table`（persist_catalog 时机之一）；若无 7a，先实现 7b 再补 drop_table 钩子。

## 实施清单（按顺序，每步可独立提交）

1. **ferrumdb-page：`Schema`/`ColumnType`/`Value` 加 serde derive**
   - `row.rs` 加 `#[derive(Serialize, Deserialize)]`；`Cargo.toml` 加 `serde = { version = "1", features = ["derive"] }`。
   - 验证：`cargo build -p ferrumdb-page` + `cargo test -p ferrumdb-page`。

2. **ferrumdb-space：superblock 扩展区 + catalog 字节读写**
   - `superblock.rs`：`OFF_CATALOG_MAGIC = 30` 常量、`catalog_bytes()` / `set_catalog_bytes()`；超限错误。
   - `Cargo.toml` 加 `serde_json`（catalog JSON 编码由 engine 完成还是 space？设计上 space 只存字节，序列化在 engine——**保持 space 只做字节存取**，避免 space 依赖 serde_json 的强耦合，或加 serde_json 到 space）。
   - 单测：round-trip、无 catalog 返回 None、超长报错。
   - 验证：`cargo test -p ferrumdb-space`。
   - **此步单独提交**（superblock 布局变更 = 回滚点）。

3. **ferrumdb-engine：catalog 序列化（JSON ↔ TableCatalog）**
   - `catalog.rs` 或新 `catalog_persist.rs`：`TableCatalog` ↔ `CatalogSnapshot`（serde_json）。
   - 单测：空 catalog / 含多表多索引 round-trip。
   - 验证：`cargo test -p ferrumdb-engine`。

4. **ferrumdb-engine：`WalPageSource`**
   - 新 `wal_source.rs`：实现 `PageSource`（包 Space + Wal），`write_page` 先 append 整页 redo（page 0 除外）。
   - 单测：经 WalPageSource 写页后 WAL 记录存在。
   - 验证：`cargo test -p ferrumdb-engine`。
   - **此步单独提交**（新接线层）。

5. **ferrumdb-engine：启动/写路径接线**
   - `open_or_create`：开 Wal、读 catalog、`wal.recover` replay、按 root 重开树。
   - 所有 btree 操作改经 `WalPageSource`；DDL 后 + insert/update/delete 后 `persist_catalog()`。
   - 验证：AC1 跨重启测试（create+insert → 重开 → 查询）。

6. **集成测试**：AC1–AC4
   - 跨重启（表 + 数据 + 索引完整）
   - 崩溃恢复（构造未刷盘 + WAL 已刷 → 重启补齐）
   - catalog 变更重启生效（drop 的表不存在、新建的表存在）
   - 根分裂 root 持久化（大量 insert → 重启 → 数据正确）

7. **验证命令**
   - `cargo build && cargo test`（全 workspace）
   - `cargo clippy --all-targets -- -D warnings`
   - 确认 `ferrumdb-sql` object-safe 测试仍绿。

8. **文档同步（finish 阶段复查）**
   - `docs/plan.md` 阶段 7 验收勾选；trait 表补持久化事实。
   - `.trellis/spec/ferrumdb-engine` 更新：WalPageSource、catalog 持久化、恢复流程。
   - `.trellis/spec/ferrumdb-space` 更新：superblock 扩展区布局。
   - 父任务 `phase7-engine` prd 勾选 7b 完成项。

9. **Review 门**：`trellis-check` 本任务变更。

## 风险文件 / 回滚点

| 文件 | 风险 | 回滚 |
|------|------|------|
| `crates/ferrumdb-space/src/superblock.rs` | 布局变更，中 | 步骤 2 单独提交，可 revert |
| `crates/ferrumdb-page/src/row.rs` | serde derive，低 | revert |
| `crates/ferrumdb-engine/src/wal_source.rs` | 新接线，中 | 步骤 4 单独提交 |
| `crates/ferrumdb-engine/src/ferrum_engine.rs` | 启动/写路径改造，中 | revert 至步骤 4 前 |
| 依赖（serde/serde_json） | 低 | revert Cargo.toml |

## start 前检查

- [ ] prd.md AC 全部可测，无阻塞 OQ
- [ ] design.md 无未决设计
- [ ] implement.jsonl / check.jsonl 各含 ≥1 条真实 spec 条目
- [ ] 用户已批准最终规划摘要（brainstorm 门）
- [ ] `task.py start` 后再进入实现
