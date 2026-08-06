# 阶段 6：二级索引与回表 — 执行计划

## 基线

- `cargo build` / `cargo test` 全绿（当前 85 测试）。
- 已读：btree persistent.rs / page row.rs / space superblock.rs / engine engine.rs。

## 实施清单（按顺序，每步可独立提交）

1. **ferrumdb-page：新增 `src/key.rs` 保序编码**
   - `encode_key_value(Value, ColumnType)`、`encode_pk`、`encode_secondary_key`（含 `∥ terminator ∥ pk`）、对应 decode。
   - 单测：i64 负/正/极值保序、Bytes 含 `0x00` escape、复合分隔边界、round-trip。
   - `lib.rs` 导出 `pub mod key;` → `cargo build -p ferrumdb-page` + `cargo test -p ferrumdb-page`。

2. **ferrumdb-engine：trait 扩展**
   - `engine.rs` 定义 `IndexMeta`；trait 新增 `create_index` / `get_by_index` / `scan_index`（object-safe）。
   - 更新方法 doc 的"实现阶段"表；`StubEngine` 补 3 个 `Unsupported`。
   - 冒烟：`cargo build -p ferrumdb-engine` + `cargo test -p ferrumdb-sql`（object-safe 仍过）。
   - **此步单独提交**（trait 变更 = 回滚点）。

3. **ferrumdb-engine：新增 `catalog.rs`**
   - `TableMeta` / `IndexEntry` / `TableCatalog`（内存 HashMap，含 root 回写方法）。

4. **ferrumdb-engine：新增 `FerrumEngine`**
   - `create_table` / `create_index` / `insert`（主键冲突 + 唯一探测 + 先探测后写入 + root 回写）/ `get_by_pk` / `get_by_index`（回表）/ `scan` / `scan_index`（物化 + 回表）。
   - `update` / `delete` / `drop_table` / `begin` / `commit` / `rollback` → `Unsupported`。
   - `lib.rs` 导出；`#![deny(missing_docs)]` 全 doc。

5. **集成测试（engine 层）**：AC1–AC7
   - 点查 / 非唯一多 pk 共享 key（AC7）/ 多索引一致（AC3）/ 唯一冲突无部分写入（AC6）/ 范围扫描+回表有序（AC2）/ 存储层持久 reopen（AC4，记 root id 后重建 engine + `PersistentBtree::open`）。

6. **验证命令**
   - `cargo build && cargo test`（全 workspace）
   - `cargo clippy --all-targets -- -D warnings`
   - 确认 `ferrumdb-sql` object-safe 测试仍绿。

7. **文档同步（finish 阶段复查）**
   - `docs/plan.md` "StorageEngine trait 方法清单"：补 `create_index`(6) / `get_by_index`(6) / `scan_index`(6)。
   - `engine.rs` doc 实现阶段表同步。
   - `.trellis/spec/ferrumdb-engine` 更新阶段 6 事实（FerrumEngine 最小实现存在）；`ferrumdb-page` 补 key 模块说明。

8. **Review 门**：`trellis-check` 本任务变更。

## 风险文件 / 回滚点

| 文件 | 风险 | 回滚 |
|------|------|------|
| `crates/ferrumdb-page/src/key.rs` | 新增，低 | 删除 + lib.rs 去导出 |
| `crates/ferrumdb-engine/src/engine.rs` | trait 变更，中 | 步骤 2 单独提交，可 revert |
| `crates/ferrumdb-engine/src/{catalog.rs,ferrum_engine.rs}` | 新增，低 | 删除 |
| `docs/plan.md` / spec | 文档 | revert |

## start 前检查

- [ ] prd.md AC 全部可测，无阻塞 OQ
- [ ] design.md 无未决设计
- [ ] implement.jsonl / check.jsonl 各含 ≥1 条真实 spec 条目（已替换 `_example`）——本任务为 sub-agent dispatch
- [ ] 用户已批准最终规划摘要（brainstorm 门）
- [ ] `task.py start` 后再进入实现
