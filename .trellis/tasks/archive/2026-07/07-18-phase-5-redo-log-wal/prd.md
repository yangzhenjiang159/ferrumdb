# 阶段 5 — Redo Log (WAL)

## Goal

按 `docs/plan.md` 阶段 5 要求实现 WAL，达到 **M1 里程碑**：

- 单文件表空间（✅ 阶段 3）
- 聚簇 B+Tree（✅ 阶段 3）
- Buffer Pool（✅ 阶段 4）
- **Redo 恢复**（⏳ 本阶段）

## Requirements

### R1 — WAL 文件格式

```
+--------------------+
| next_lsn: u64 LE   |  <- 8 字节固定头
+--------------------+
| record_0           |
| record_1           |
| ...                |
| record_N           |
+--------------------+
| (optional) checkpoint:
|   magic: 0xFEEDC0DE u32 LE
|   max_flushed_lsn: u64 LE
+--------------------+
```

Record：
```
[lsn: u64 LE]
[page_id: u32 LE]
[offset: u32 LE]
[payload_len: u32 LE]
[payload: payload_len bytes]
[crc32: u32 LE]   <- over (lsn || page_id || offset || payload_len || payload)
```

### R2 — Wal API

```rust
pub struct Wal { ... }
pub struct RedoRecord { lsn, page_id, offset, payload }

impl Wal {
    pub fn create(path) -> Result<Self, WalError>;
    pub fn open(path) -> Result<Self, WalError>;
    pub fn append(&mut self, page_id, offset, payload) -> Result<u64, WalError>;
    pub fn fsync(&mut self) -> Result<(), WalError>;
    pub fn checkpoint(&mut self, max_flushed_lsn: u64) -> Result<(), WalError>;
    pub fn checkpoint_lsn(&self) -> u64;
    pub fn next_lsn(&self) -> u64;
    pub fn recover<S: WritePage>(&mut self, target: &mut S) -> Result<u64, WalError>;
}
```

### R3 — Error type

```rust
pub enum WalError {
    Io(#[from] std::io::Error),
    RecordCrcMismatch { lsn: u64 },
    Truncated,                       // log file ends mid-record (treat as EOF, not error)
    InvalidRecord(String),
    CheckpointCorrupt,
    LsnExhausted,
    OutOfOrder { expected: u64, got: u64 },
}
```

### R4 — Recovery

`recover(target)` 从 `checkpoint_lsn + 1` 开始顺序读 record，对每条 record：
- 读 target 的 page（如果不存在则跳过，记录 warning）
- 在 offset 处写入 payload
- 写回 page

返回已 replay 的最大 lsn。`Truncated` 不算 error（视为正常 EOF）。

### R5 — 测试

- [x] append + reopen + recover 一致
- [x] checkpoint 后的 record 不被 replay
- [x] 多条 record 顺序应用
- [x] truncate log file → recover 仍能 replay 已写入的 record
- [x] 模拟 kill 进程（drop WAL + Space 不 flush）→ reopen + recover → 数据完整
- [x] CRC mismatch 返回 error
- [x] `Truncated` 不算 error

## Acceptance Criteria

- [x] R1: WAL 文件格式正确（little-endian，8 字节头，可变长 record）
- [x] R2: 所有 API 都能编译并通过测试
- [x] R3: 每个 WalError 变体可达测试
- [x] R4: recover 正确从 checkpoint 之后开始 replay
- [x] R5: kill-after-write 测试通过（关键的 M1 测试）
- [x] `cargo build` 无 warning；`cargo test` 全过；`cargo clippy` 干净
- [x] 不破坏阶段 1-4 的 71 个测试
- [x] `#![deny(missing_docs)]` 仍启用
- [x] spec 同步：`ferrumdb-wal/backend/` 6 文件

## Constraints

- 不引入新外部依赖（`crc32fast` 已有 via ferrumdb-page）
- 不引入 `unsafe`
- 不引入 `async`
- 不修改 ferrumdb-buffer / ferrumdb-btree / ferrumdb-space 的核心 API
- v1 简化：append 不重写 header（只在 create / open 时读写一次）

## Out of Scope

- WAL ↔ BufferPool 自动集成（v1 手动调用 wal.append）
- Undo log（阶段 9）
- Group commit / batch fsync
- Multi-WAL thread 安全

## References

- `docs/plan.md` 阶段 5
- `docs/architecture.md` storage_layer (Buffer ↔ WAL)
- `.trellis/spec/ferrumdb-wal/backend/`（待填充）
