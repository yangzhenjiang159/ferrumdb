# FerrumDB 实测代码模式（2026-07-18）

> 这是 `.trellis/spec/` 所有包规范文件的**唯一事实底座**。本笔记由主会话基于对 `crates/` 全部源码的扫描得出，所有 worker 必须以此为起点，不要凭空发明模式。

## 0. 仓库现状快照

| 维度 | 实测值 |
|------|--------|
| Crate 数 | 10（全部已 `cargo build` 通过） |
| 已实现代码 | 仅 `ferrumdb-page`（完整 Page 布局 + checksum）和 `ferrumdb-engine`（仅 trait 骨架） |
| 其余 8 crate | `lib.rs` 占位 + `#![deny(missing_docs)]` + 一个 `crate_compiles` 空测试 |
| `cargo test` 状态 | 全部通过（10 个 `crate_compiles` + page.rs 7 个测试 + engine 0 个） |
| `cargo build` | 通过，无 warning |

> **关键事实**：大部分 crate 只有**约定框架**（文档注释 + Cargo 元数据 + 占位测试），没有真实业务代码。spec 文件必须**诚实记录这一现状**，包括"未来阶段才会实现的接口"，不能用虚构的实现细节填充。

## 1. 全局 Cargo 约定（来自根 `Cargo.toml` + 10 个子 `Cargo.toml`）

### 1.1 Workspace 元数据
- `[workspace]` `resolver = "2"`
- `[workspace.package]`：`version = "0.1.0"`, `edition = "2021"`, `license = "MIT OR Apache-2.0"`
- 子 crate 全部 `version.workspace = true`、`edition.workspace = true`、`license.workspace = true`

### 1.2 外部依赖（统一在 `[workspace.dependencies]`）
- `thiserror = "2"` —— 所有 crate 都用
- `anyhow = "1"` —— 仅 `ferrumdb-server` 用
- `bytes = "1"` —— `ferrumdb-page` + `ferrumdb-protocol`
- `crc32fast = "1"` —— 仅 `ferrumdb-page`
- `tokio = { version = "1", features = ["full"] }` —— 仅 `ferrumdb-server`
- `tracing = "0.1"` + `tracing-subscriber = "0.3"` —— 仅 `ferrumdb-server`

**规则**：所有依赖都通过 `xxx.workspace = true` 引用，禁止在子 crate 直接写版本号。

### 1.3 每个子 crate 的 `Cargo.toml` 必含字段
```toml
[package]
name = "ferrumdb-<x>"
description = "<一句话职责>"      # 必填，对应 crate 实际用途
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
# 仅列该 crate 真正用到的内部 + 外部依赖
```
- `ferrumdb-server` 是唯一带 `[[bin]]` 的 crate（`name = "ferrumdb-server"`, `path = "src/main.rs"`）
- 其余都是 lib-only

### 1.4 内部依赖分层（实测 `Cargo.toml` 推导）

| 层 | Crate | 依赖 |
|---|---|---|
| L0（无内部依赖） | `ferrumdb-page`, `ferrumdb-protocol` | 仅外部 |
| L1 | `ferrumdb-btree`, `ferrumdb-space`, `ferrumdb-wal` | `ferrumdb-page` |
| L2 | `ferrumdb-buffer`, `ferrumdb-txn` | `page` + (`space` 或 `wal`) |
| L3 | `ferrumdb-engine` | `page` + `btree` + `buffer` + `wal` + `space` + `txn` |
| L4 | `ferrumdb-sql` | `engine` + `page` |
| L5 | `ferrumdb-server` | `engine` + `protocol` + `sql` + `tokio` + `tracing` + `anyhow` |

**铁律**：依赖只能向下，不许反向依赖。`ferrumdb-page` 是最底层，被 7 个 crate 引用。

## 2. 模块级文档约定（实测所有 `lib.rs`）

每个 `src/lib.rs` 顶部必须有以下结构的 `//!` 模块文档：

```rust
//! <crate 中文名一句话>。
//!
//! # 职责
//!
//! - <职责要点 1>
//! - <职责要点 2>
//! - ...
//!
//! 见项目文档 `docs/plan.md` 阶段 <N>。
```

并且必须 `#![deny(missing_docs)]`（10 个 crate 全部启用，禁止任何公开项缺文档注释）。

## 3. 错误处理约定（实测 `ferrumdb-page/src/error.rs` + `ferrumdb-engine/src/engine.rs`）

### 3.1 标准模式
```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum XxxError {
    #[error("<可读消息，可能带 {0}>")]
    Variant1(Type1),
    #[error("variant 2 message")]
    Variant2,
}
```
- `thiserror::Error` derive，**不是**手写 `impl std::error::Error`
- 错误消息用**中文**（仓库注释一致风格）
- 变体命名：领域术语（`InvalidLength`、`ChecksumMismatch`、`TableNotFound`、`DuplicateKey`、`Unsupported`、`Internal`），避免 `Err1`/`ErrOther`
- `Internal(String)` 是通用兜底（`ferrumdb-engine` 实测）

### 3.2 错误分类约定
- **可预期/可恢复**：专用变体（如 `TableNotFound`、`DuplicateKey`）
- **未实现/未来阶段**：`Unsupported(String)` —— 阶段 7 之前的 trait 方法应返回它
- **内部/系统级**：`Internal(String)` —— I/O、损坏、断言失败等

### 3.3 调用方传播
- 业务 crate **用 `Result<T, XxxError>`**，不引入 `anyhow`（仅 `ferrumdb-server` 用 `anyhow`，因为它是二进制入口）
- `?` 传播；不在中间层 `map_err` 包装成新错误（除非跨 crate 边界且语义不同）

## 4. 类型设计约定

### 4.1 公开类型字段全部 `pub`，但方法优先
- 简单值类型（`Row.values`、`Schema.columns`、`PageHeader.page_id`）字段 `pub`
- 有不变量的类型（`Page`）封装字段 + 提供 getter 方法（`page_id()`、`page_type()`、`header()`）

### 4.2 `repr(u8)` 用于磁盘枚举
- `PageType` 用 `#[repr(u8)]` 配合 `to_u8() / from_u8() -> Result<_, PageError>` 的双转换（参考 `crates/ferrumdb-page/src/page.rs:60-91`）

### 4.3 trait 设计
- `StorageEngine` trait 在 `ferrumdb-engine/src/engine.rs`：
  - 方法签名含 doc comment + `# Errors` 段列出可能错误变体
  - 含"实现阶段"表格说明每个方法在哪一阶段实现
  - 关联类型用 `type RowIterator<'a>`，实现成 `Box<dyn Iterator<...> + 'a>`

## 5. 字节序与编码（实测 `page.rs` 文档）

- 全项目统一 **little-endian**（`page.rs:6` 明确写出）
- `PAGE_MAGIC = 0xFEDB_0001` 用于识别 FerrumDB 页
- `PAGE_SIZE = 16384`、`PAGE_HEADER_SIZE = 32`、`PAGE_FOOTER_SIZE = 8`
- 任何新增的磁盘格式必须在模块文档画字节布局图（参考 `page.rs:8-22` 的 ASCII 图风格）

## 6. 测试约定

### 6.1 占位测试（8 个 stub crate 共用）
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
```
目的：确保 crate 自身可编译、不被 `#![deny(missing_docs)]` 拒绝。

### 6.2 `ferrumdb-page` 真实测试模式（实测）
- 测试名 snake_case 描述行为：`page_round_trip`、`page_checksum_detects_corruption`、`page_invalid_length`、`page_invalid_magic`、`page_type_round_trip`
- 字段断言用 `assert_eq!`，不变量用 `assert_ne!`
- 异常路径用 `assert_eq!(Page::from_bytes(&bad), Err(PageError::ChecksumMismatch))`
- 字节级测试中：`bytes[100] ^= 0xFF` 翻转单字节验证检测能力

### 6.3 测试范围
- happy path + 至少一个 corruption / invalid input 用例
- 不引入 mock 框架（目前为止没用 mockall 等）

## 7. 日志约定（仅 `ferrumdb-server` 实测）

```rust
fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("FerrumDB server — not implemented yet (see docs/plan.md phase 8)");
}
```
- **库 crate 不输出日志**（page/btree/... 都不调 tracing）
- 仅 server 入口初始化 subscriber + 用 `tracing::info!`
- 日志级别未在 crate 间显式分级，目前所有日志都用 `info!`

## 8. 目录结构约定（实测）

```
crates/<crate-name>/
├── Cargo.toml
└── src/
    ├── lib.rs           # 模块文档 + #![deny(missing_docs)] + 重导出
    ├── error.rs         # 错误类型（每个有错误的 crate 一个）
    └── <其它模块>.rs    # 按职责拆分（page 有 page.rs + row.rs；engine 有 engine.rs）
```
- 错误类型**单独成文件** `error.rs`，不要塞进 `lib.rs`
- 模块可见性：`mod error;`、`mod page;`、`mod row;` 全小写无前缀
- `pub use` 在 `lib.rs` 顶层重导出所有公开 API（参考 `ferrumdb-page/src/lib.rs:9-16`）

## 9. 文档交叉引用约定

- 模块文档必须指向 `docs/plan.md` 对应阶段（10 个 lib.rs 全部含此句）
- 公开类型/方法用 doc comment `///`，关键术语加反引号（如 `[`Page`]`、`[`PAGE_SIZE`]`）
- 不在 Rust doc 里写中文版"示例"代码 —— 用纯中文解释 + 引用真实路径

## 10. Review 检查清单（来自 `docs/plan.md` 末段，spec 的 Quality Check 直接复用）

- 是否满足该阶段验收标准
- 错误类型是否用 `thiserror` 表达清晰
- 公开 API 是否有 doc comment
- 单元/集成测试是否覆盖 happy path + 关键 failure
- 是否与下一阶段接口对齐（见 `docs/architecture.md`）

## 11. 严禁模式（基于代码反向归纳）

- ❌ 在子 crate `Cargo.toml` 直接写版本号（必须 `xxx.workspace = true`）
- ❌ 用 `unwrap()` / `expect()` 在业务 crate（非测试、非 server main）
- ❌ 手写 `impl std::error::Error`（用 `thiserror` derive）
- ❌ 库 crate 用 `println!` / `eprintln!`（用 `tracing`）
- ❌ 跨层级反向依赖（如 `ferrumdb-page` 引用 `ferrumdb-engine`）
- ❌ 公开 API 缺 doc comment（被 `#![deny(missing_docs)]` 拒）
- ❌ 在 `lib.rs` 里塞实现（每个职责一个文件，如 `error.rs`、`page.rs`）
- ❌ 错误消息用大段英文（仓库风格是简短中文 + 必要时占位符 `{0}`）

## 12. spec 文件写作要点（给 worker 的指南）

1. **目录结构**：每 crate 6 个文件：`index.md`（入口 + Pre-Dev Checklist + Quality Check）、`directory-structure.md`、`database-guidelines.md`、`error-handling.md`、`logging-guidelines.md`、`quality-guidelines.md`
2. **现实主义**：只描述代码实际做的事。stub crate 写"职责规划 + 占位测试 + 暂未实现"，不要伪造实现示例
3. **真实路径引用**：每个代码示例必须标 `crates/<x>/src/<file>.rs:<行号>`
4. **Pre-Dev Checklist**：用 checklist 形式列出本 crate 开工前要确认的事项
5. **Quality Check**：把 §10 的 Review 检查清单具体化为本 crate 的可勾选项
6. **语言**：**英文**（遵循 prd.md "Language: English" 要求）
7. **目录结构文件**：列出实际目录 + 标注每个文件用途；标记"尚未存在"的规划文件
