# 阶段 7a：DML 补全 — 执行计划

## 基线

- `cargo build` / `cargo test` 全绿（当前 110 测试）；`cargo clippy --all-targets -- -D warnings` 干净。
- 已读：btree persistent.rs / engine engine.rs + ferrum_engine.rs + catalog.rs / space space.rs。

## 实施清单（按顺序，每步可独立提交）

1. **ferrumdb-btree：修复 `delete` 叶子链遍历**
   - `persistent.rs::delete`：镜像 `get` 的 `next_leaf` 遍历逻辑（design §2）。
   - 单测：插入 > ORDER 键数触发多层分裂，逐个删除所有 key 后 `len()==0` 且 `scan_range` 为空；删除不存在的 key 返回 `Ok(false)`。
   - 验证：`cargo build -p ferrumdb-btree` + `cargo test -p ferrumdb-btree`。
   - **此步单独提交**（btree 行为变更 = 回滚点）。

2. **ferrumdb-btree：新增 `all_node_page_ids`**
   - BFS/DFS 遍历返回全部节点页 id（design §6）。
   - 单测：建树插入若干键后，返回 id 集合与 root/叶子/内部页一致；去重。
   - 验证：`cargo test -p ferrumdb-btree`。
   - **此步单独提交**。

3. **ferrumdb-engine：`EngineError::RowNotFound`**
   - `engine.rs` 新增变体 + doc。
   - 验证：`cargo build -p ferrumdb-engine` + `cargo test -p ferrumdb-sql`（object-safe 仍过）。

4. **ferrumdb-engine：`update` / `delete` / `drop_table`**
   - `ferrum_engine.rs` 实现三方法（design §4-6）。
   - `update` 复用 insert 的唯一索引探测路径（先探测后写）。
   - 验证：`cargo build -p ferrumdb-engine` + 步骤 5 的集成测试。

5. **集成测试（engine 层）**：AC1–AC5
   - update 索引列不变/变、update 不存在行 → RowNotFound、delete 幂等、drop_table 释放页复用、btree 高分裂删除不漏删。
   - 验证：`cargo test -p ferrumdb-engine`。

6. **验证命令**
   - `cargo build && cargo test`（全 workspace）
   - `cargo clippy --all-targets -- -D warnings`
   - 确认 `ferrumdb-sql` object-safe 测试仍绿。

7. **文档同步（finish 阶段复查）**
   - `docs/plan.md` trait 方法表：`update`/`delete`/`drop_table` 阶段 7 已实现。
   - `.trellis/spec/ferrumdb-engine` 更新：FerrumEngine 现已实现 update/delete/drop_table；`EngineError::RowNotFound` 变体。
   - `.trellis/spec/ferrumdb-btree`：delete 叶子链遍历修复 + `all_node_page_ids` 事实。
   - 父任务 `phase7-engine` prd 勾选 7a 完成项。

8. **Review 门**：`trellis-check` 本任务变更。

## 风险文件 / 回滚点

| 文件 | 风险 | 回滚 |
|------|------|------|
| `crates/ferrumdb-btree/src/persistent.rs` | delete 行为变更，中 | 步骤 1/2 各自单独提交，可 revert |
| `crates/ferrumdb-engine/src/engine.rs` | 新增错误变体，低 | revert |
| `crates/ferrumdb-engine/src/ferrum_engine.rs` | 新增方法实现，低 | 删除方法体回退 Unsupported |
| `crates/ferrumdb-engine/src/integration.rs` | 测试，低 | 删除 |

## start 前检查

- [ ] prd.md AC 全部可测，无阻塞 OQ
- [ ] design.md 无未决设计
- [ ] implement.jsonl / check.jsonl 各含 ≥1 条真实 spec 条目
- [ ] 用户已批准最终规划摘要（brainstorm 门）
- [ ] `task.py start` 后再进入实现
