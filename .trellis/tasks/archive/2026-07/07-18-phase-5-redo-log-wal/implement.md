# Implement — 阶段 5 Redo Log (WAL)

## Step 1 — error.rs + record.rs
- [ ] WalError 7 变体
- [ ] RedoRecord struct + encode_record (writes lsn/page_id/offset/payload/crc32)
- [ ] decode_record (parses same, validates crc32)

## Step 2 — wal.rs
- [ ] Wal struct
- [ ] Wal::create + Wal::open (read header + scan records for next_lsn + checkpoint)
- [ ] Wal::append (encode + write + fsync + bump lsn)
- [ ] Wal::checkpoint (write magic + max_flushed_lsn)
- [ ] Wal::fsync
- [ ] Wal::recover (replay records after checkpoint to a target writer)
- [ ] TruncateRecord handling in open() and recover()

## Step 3 — lib.rs
- [ ] 重导出 Wal, RedoRecord, WalError
- [ ] #![deny(missing_docs)]
- [ ] 6+ unit tests

## Step 4 — 关键集成测试 (M1 达标)
- [ ] 创建 WAL + Space，分配 page 1
- [ ] 修改 page 1 内容，append WAL record, fsync
- [ ] **drop WAL 和 Space（模拟进程 kill）**
- [ ] 重新打开 WAL + Space
- [ ] 调用 recover
- [ ] 读 page 1，验证内容是修改后的版本

## Step 5 — Validation
- [ ] cargo test 全过 (71 + 8 new = 79)
- [ ] cargo clippy 0 warning

## Step 6 — Spec 同步
- [ ] 6 个 ferrumdb-wal spec 文件填充

## Step 7 — Archive
- [ ] finish + archive
