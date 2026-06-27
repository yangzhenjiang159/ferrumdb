# FerrumDB 依赖说明

根目录 [`Cargo.toml`](../Cargo.toml) 使用 Cargo **workspace** 统一管理子 crate 与外部依赖版本。子 crate 通过 `xxx.workspace = true` 引用 `[workspace.dependencies]` 中定义的包。

---

## Workspace 配置

| 字段 | 说明 |
|------|------|
| `[workspace]` | 将多个 crate 置于同一仓库，统一构建与依赖版本 |
| `resolver = "2"` | Cargo 2021 依赖解析策略；feature 传递更合理，workspace 项目推荐使用 |
| `members` | 10 个子 crate 路径；根目录执行 `cargo build` / `cargo test` 时会一并编译 |

---

## 公共包元数据 `[workspace.package]`

子 crate 中 `version.workspace = true` 等字段会继承此处定义：

| 字段 | 当前值 | 说明 |
|------|--------|------|
| `version` | `0.1.0` | 全 workspace 统一版本 |
| `edition` | `2021` | Rust 2021 语法与标准库特性 |
| `license` | `MIT OR Apache-2.0` | 双许可证，与 Rust 生态常见做法一致 |
| `authors` | FerrumDB Contributors | 作者信息 |
| `repository` | GitHub URL（待填写） |  crates.io 与文档中的源码链接 |

---

## 外部依赖 `[workspace.dependencies]`

### `thiserror = "2"`

**用途**：在库 crate 中定义结构化错误枚举。

**选用原因**：FerrumDB 各模块需要明确的错误类型（如 `PageError::ChecksumMismatch`），并实现 `std::error::Error`，便于上层匹配与传播。

**使用 crate**：

- `ferrumdb-page`
- `ferrumdb-btree`
- `ferrumdb-buffer`
- `ferrumdb-wal`
- `ferrumdb-space`
- `ferrumdb-txn`
- `ferrumdb-engine`
- `ferrumdb-protocol`
- `ferrumdb-sql`

**约定**：库层用 `thiserror`；二进制入口（server）用 `anyhow` 更方便。

---

### `anyhow = "1"`

**用途**：应用层错误处理；使用 `Result<T, anyhow::Error>` 快速传播多种错误，无需为每种组合单独定义 enum。

**选用原因**：`ferrumdb-server` 作为 `main` 入口，需组装 engine、protocol、sql，错误来源多，用 `anyhow` 更简洁。

**使用 crate**：

- `ferrumdb-server`

---

### `bytes = "1"`

**用途**：高效字节缓冲区（`Bytes`、`BytesMut`、`Buf`、`BufMut`），带引用计数，适合协议解析与减少拷贝。

**选用原因**：

- MySQL Wire Protocol 为二进制包流，需按长度切包、拼包
- Page 序列化会频繁读写固定长度字节块

**使用 crate**：

- `ferrumdb-page`（页 `to_bytes` / `from_bytes`）
- `ferrumdb-protocol`（协议编解码，阶段 8）

---

### `crc32fast = "1"`

**用途**：硬件加速的 CRC32 校验和计算。

**选用原因**：阶段 1 页完整性校验；比手写循环更快，适合 16KB 整页校验。

**使用 crate**：

- `ferrumdb-page`

---

### `tokio = { version = "1", features = ["full"] }`

**用途**：Rust 异步运行时；提供 TCP、定时器、任务调度等。

**选用原因**：`ferrumdb-server` 需并发处理多个客户端连接；异步 I/O 比「每连接一线程」更省资源。

**`features = ["full"]`**：启用 tokio 全部官方 feature（`net`、`io`、`rt-multi-thread`、`time`、`fs` 等）。学习阶段省事；后续可收窄为 `["rt-multi-thread", "net", "io-util", "macros"]` 以加快编译。

**使用 crate**：

- `ferrumdb-server`（阶段 8 实现 TCP 服务）

---

### `tracing = "0.1"`

**用途**：结构化日志与追踪 API（`tracing::info!`、`debug!`、`span!` 等）。

**与 `log` crate 的区别**：支持 span（请求级上下文），便于跟踪「某条 SQL → 某次页读写 → 某条 WAL」的完整链路。

**选用原因**：数据库需观察连接、慢路径、崩溃恢复等，比 `println!` 更可控、可过滤。

**使用 crate**：

- `ferrumdb-server`（后续 engine、wal 等模块也可能接入）

---

### `tracing-subscriber = "0.3"`

**用途**：`tracing` 的订阅与输出后端，将事件格式化为人类可读日志或 JSON。

**选用原因**：在 `main` 中一行初始化即可启用日志，例如：

```rust
tracing_subscriber::fmt::init();
```

**使用 crate**：

- `ferrumdb-server`（与 `tracing` 成对使用）

---

## 依赖分布一览

| 依赖 | 类型 | 使用 crate | 主要阶段 |
|------|------|------------|----------|
| `thiserror` | 错误定义 | 几乎所有库 crate | 全程 |
| `anyhow` | 错误传播 | `ferrumdb-server` | 8 |
| `bytes` | 字节缓冲 | `page`, `protocol` | 1, 8 |
| `crc32fast` | CRC32 校验 | `page` | 1 |
| `tokio` | 异步运行时 | `ferrumdb-server` | 8 |
| `tracing` | 日志 API | `ferrumdb-server` | 8+ |
| `tracing-subscriber` | 日志输出 | `ferrumdb-server` | 8+ |

存储引擎内核（page → btree → buffer → wal → space → txn → engine）目前主要依赖 **`thiserror`** 与 **`bytes`**（page），尚未引入 async 或日志 crate，符合「先做好同步内核，再做网络层」的顺序。

---

## 内部 crate 依赖关系

子 crate 之间通过 `path = "../..."` 引用，不在 `[workspace.dependencies]` 中声明：

```text
ferrumdb-server
  └── ferrumdb-engine, ferrumdb-protocol, ferrumdb-sql

ferrumdb-sql
  └── ferrumdb-engine, ferrumdb-page

ferrumdb-engine
  └── ferrumdb-page, ferrumdb-btree, ferrumdb-buffer,
      ferrumdb-wal, ferrumdb-space, ferrumdb-txn

ferrumdb-txn
  └── ferrumdb-page, ferrumdb-wal

ferrumdb-wal / ferrumdb-buffer
  └── ferrumdb-page, ferrumdb-space

ferrumdb-btree / ferrumdb-space
  └── ferrumdb-page
```

详见 [architecture.md](architecture.md)。

---

## 后续可能引入的依赖

以下尚未加入 `Cargo.toml`，将在对应阶段按需添加：

| 包 | 可能用途 | 阶段 |
|----|----------|------|
| `crc32fast` | Page checksum 计算 | 1 |
| `tempfile` | 集成测试中的临时表空间文件 | 3+ |
| `parking_lot` | Buffer Pool 互斥锁（比 std 更轻） | 4 |
| `sqlparser` | SQL 解析（可选；也可手写 parser） | 8 |

新增依赖时：

1. 在根 `Cargo.toml` 的 `[workspace.dependencies]` 中声明版本
2. 在子 crate 中用 `xxx.workspace = true` 引用
3. 更新本文档

---

## 变更记录

| 日期 | 说明 |
|------|------|
| 2025-06-27 | 初版，说明 workspace 与 6 个外部依赖 |
