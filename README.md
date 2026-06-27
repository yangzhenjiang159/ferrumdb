# FerrumDB

> A from-scratch InnoDB-style storage engine in Rust.

FerrumDB 是一个学习导向的数据库项目：从零理解 MySQL 如何运行，并用 Rust 实现一套与 InnoDB 设计思想一致的存储引擎（页、B+Tree、Buffer Pool、Redo/Undo、MVCC）。

## 项目目标

1. **彻底理解 MySQL**：连接层 → SQL 解析 → 优化/执行 → 存储引擎接口 → 磁盘。
2. **实现 InnoDB 风格引擎**：以 16KB 页为基本单位，聚簇索引 + 二级索引，WAL 崩溃恢复，逐步引入 MVCC。
3. **协作方式**：开发步骤与接口设计见文档；**代码由开发者手写**，完成后进行 Review 与改进。

## 文档

| 文档 | 说明 |
|------|------|
| [docs/plan.md](docs/plan.md) | 分阶段开发计划、里程碑与验收标准 |
| [docs/architecture.md](docs/architecture.md) | 系统架构与 crate 职责 |
| [docs/collaboration.md](docs/collaboration.md) | 协作与 Code Review 约定 |
| [docs/dependencies.md](docs/dependencies.md) | Workspace 与外部依赖说明 |

## Workspace 结构

```
ferrumdb/
├── crates/
│   ├── ferrumdb-page/      # 16KB 页格式与行编码
│   ├── ferrumdb-btree/     # B+Tree 索引
│   ├── ferrumdb-buffer/    # Buffer Pool
│   ├── ferrumdb-wal/       # Redo Log 与崩溃恢复
│   ├── ferrumdb-space/     # 表空间与页分配
│   ├── ferrumdb-txn/       # 事务、Undo、MVCC
│   ├── ferrumdb-engine/    # StorageEngine 统一入口
│   ├── ferrumdb-protocol/  # MySQL Wire Protocol（后期）
│   ├── ferrumdb-sql/       # SQL 解析与执行（后期）
│   └── ferrumdb-server/    # TCP 服务（后期）
└── docs/
```

## 当前阶段

**阶段 2 — 行编码 + 内存 B+Tree**（见 [docs/plan.md#阶段-2--行编码--内存-btree](docs/plan.md#阶段-2--行编码--内存-btree)）

下一步：在 `ferrumdb-page` 完善行编解码，在 `ferrumdb-btree` 实现内存 B+Tree。

## 构建

```bash
cargo build
cargo test
```

## 许可证

MIT OR Apache-2.0
