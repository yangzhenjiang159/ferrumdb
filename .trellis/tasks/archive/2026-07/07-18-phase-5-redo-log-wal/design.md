# Design — 阶段 5 Redo Log (WAL)

## 1. 模块依赖

```
ferrumdb-page ← ferrumdb-wal ← ferrumdb-space (recovery target)
```

WAL 依赖 ferrumdb-page（用 PAGE_SIZE 常量；可选）和 ferrumdb-space（SpaceError 通过 PageSource 间接传播）。

## 2. 文件格式

**Header** (offset 0-7): `next_lsn: u64 LE`

**Record** (变长):
```
[lsn:        u64 LE]  8 bytes
[page_id:    u32 LE]  4 bytes
[offset:     u32 LE]  4 bytes
[payload_len:u32 LE]  4 bytes
[payload:    N bytes]
[crc32:      u32 LE]  4 bytes (over lsn || page_id || offset || payload_len || payload)
```

**Checkpoint** (optional, append at end):
```
[magic:           u32 LE = 0xFEEDC0DE]  4 bytes
[max_flushed_lsn: u64 LE]              8 bytes
```

## 3. Wal 结构

```rust
pub struct Wal {
    file: File,
    path: PathBuf,
    next_lsn: u64,            // 下一个要分配的 lsn
    checkpoint_lsn: u64,      // 上次 checkpoint 时的 max_flushed_lsn
    bytes_written: u64,       // 文件总长度（用于 truncate 检测）
}
```

## 4. 关键算法

### 4.1 create(path)
1. `OpenOptions::create().truncate().write(true)` 打开
2. 写 8 字节 header (next_lsn=1, initial state)
3. fsync
4. next_lsn = 1, checkpoint_lsn = 0

### 4.2 open(path)
1. 打开文件
2. 读 8 字节 header → next_lsn
3. 扫文件剩余部分，解析 records 直到 EOF / Truncated / 错误
4. 找 checkpoint record（如果在末尾）→ checkpoint_lsn
5. 验证：所有 record 的 lsn 必须 < next_lsn（否则文件损坏）

### 4.3 append(page_id, offset, payload)
1. lsn = next_lsn
2. 编码 record (lsn, page_id, offset, payload, crc32)
3. write_all + fsync
4. next_lsn += 1
5. 返回 lsn

### 4.4 checkpoint(max_flushed_lsn)
1. 写 checkpoint record (magic, max_flushed_lsn)
2. fsync
3. checkpoint_lsn = max_flushed_lsn

### 4.5 recover(target)
1. 重新打开文件（如果还没）
2. seek to header (offset 0)
3. 跳过 8 字节 header
4. 循环读 record：
   - 如果 record.lsn > checkpoint_lsn：replay
   - 如果是 Truncated（文件末尾 mid-record）：break
5. 返回最大 replay lsn

## 5. 与 B+Tree / BufferPool 的集成

**v1 简化**：不自动集成。测试 / 调用方需要手动：
1. `wal.append(page_id, 0, &new_page_content)` 
2. `wal.fsync()`
3. 修改 page（通过 BufferPool）

完整 write-through 集成（wal 拦截 page_mut）放到 v2。

## 6. 错误处理

- `RecordCrcMismatch`: 单条 record 损坏 → 终止 replay，返回错误
- `Truncated`: 文件末尾 mid-record → 视为正常 EOF（可能正在写）
- `OutOfOrder`: record.lsn 与 next_lsn 不一致 → 文件损坏
- 其他 I/O: 直接传播
