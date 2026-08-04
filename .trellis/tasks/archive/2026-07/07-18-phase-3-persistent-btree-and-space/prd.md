# 阶段 3 — 持久化 B+Tree + 表空间

## Goal

按 `docs/plan.md` 阶段 3 要求，实现：

1. **`ferrumdb-space`**：表空间文件管理
   - `Space::open(path)` / `Space::create(path)`
   - Superblock（page 0）：magic、version、page_size、free_list_head
   - `read_page(page_id) -> Result<Page, SpaceError>`
   - `write_page(page_id, &Page) -> Result<(), SpaceError>`
   - `allocate_page() -> Result<PageId, SpaceError>`（free list → extend file）
   - `free_page(page_id) -> Result<(), SpaceError>`
   - 每次 metadata 写后 fsync
2. **`ferrumdb-btree`**：持久化 B+Tree
   - `PersistentBTree<K, V>` 持有 `&mut Space`
   - 节点 spill 到 `PageType::Index` 页
   - 拆分后新页 `write_page` 持久化
   - `flush()` 把所有 dirty 页落盘
   - 重启后能完整恢复树结构
3. **集成测试**：建表空间 → 创建 PersistentBTree → 插入 N key → drop → 重新打开 → 验证全部 key 存在

## Requirements

### R1 — Space 文件管理

| ID | 要求 |
|----|------|
| R1.1 | `PAGE_SIZE = 16384`，从 `ferrumdb_page` 引用，**不重新定义** |
| R1.2 | `file_offset_of(page_id) = page_id * PAGE_SIZE`（唯一计算点） |
| R1.3 | `Space::create(path)` 写入 superblock + magic = `PAGE_MAGIC` |
| R1.4 | `Space::open(path)` 读 + 校验 magic；失败返回 `SpaceError::SuperblockInvalidMagic` |
| R1.5 | Superblock 持久化在 page 0 的 user_data（page header 自带 magic 校验） |
| R1.6 | `Space::sync_all()` 每次 metadata 操作后调用 |
| R1.7 | `Space::close()` 落盘 + drop file handle |

### R2 — Page 分配

| ID | 要求 |
|----|------|
| R2.1 | Free list 是 `PageType::Free` 页的链表（user_data 存 `next_page_id`） |
| R2.2 | `allocate_page` 优先从 free list 取；空则 `set_len` 扩展 1 页 |
| R2.3 | `free_page(id)` 把 page 标记为 Free 并加入链表头 |
| R2.4 | 分配 / 释放必须 fsync 后才返回 Ok |

### R3 — PersistentBTree

| ID | 要求 |
|----|------|
| R3.1 | `PersistentBTree<K, V>` 持有 `&mut Space`（借用，调用方拥有 Space） |
| R3.2 | 节点 = 一个 `Page`；user_data 存 kind byte + key count + key/value 字节 |
| R3.3 | Root page id 单独存（v1 简化：写死在 superblock 的预留区） |
| R3.4 | `insert(key, value)` 拆分后 `space.write_page(new_id, page)` |
| R3.5 | `get(key)` 读取 root → 递归 → leaf |
| R3.6 | `scan_range(start, end)` 复用 phase-2 `ScanIter` 模式（v1 简化为一次返回 Vec） |
| R3.7 | `flush()` 把所有 dirty page 落盘（v1 简化为：每次 insert 后立即 fsync） |
| R3.8 | 通用约束：`K: Ord + Clone + Serialize`, `V: Clone + Serialize`（v1 手写二进制编码，不引 serde） |

### R4 — 错误处理

新增 `SpaceError`：
- `Io(#[from] std::io::Error)`
- `PageIdOutOfRange(PageId)`
- `FreeListCorrupted(PageId)`
- `SuperblockInvalidMagic`
- `SuperblockPageSizeMismatch { file, build }`
- `NotInitialized`（未 open / 未 create 的 Space 调用 allocate_page）

### R5 — 测试

| 测试 | 场景 |
|------|------|
| space_create_then_open | create → close → open，superblock 完整 |
| space_open_bad_magic | 写入坏 magic，open 返回 `SuperblockInvalidMagic` |
| space_allocate_and_free | 分配 → 写 → 读回 → free → 再分配拿到同 id |
| space_free_list_integrity | free list 链路正确（next pointer 闭环） |
| space_extend_file | allocate_page 触发 set_len 扩展文件 |
| persistent_insert_get_round_trip | insert + get |
| persistent_1000_keys_reopen | 插入 1000 keys → drop → 重新打开 → 全 key 存在 |
| persistent_split_persists | 强制多次 root split → 重启 → 高度正确 + 数据完整 |
| persistent_scan_range | 持久化版范围扫描 |

## Acceptance Criteria

- [x] R1: create + open 双向操作；magic 校验生效
- [x] R1: superblock 写后 `sync_all`
- [x] R2: free list + 文件扩展都能正常工作
- [x] R3: insert/get 持久化后重启可读
- [x] R3: 触发根分裂后重启树高一致
- [x] R3: 1000 keys 插入 + 重启后无丢失
- [x] R4: 每个 SpaceError 变体可达测试
- [x] R5: `cargo build` 无 warning；`cargo test` 全过；`cargo clippy` 干净
- [x] 不破坏阶段 1 + 阶段 2 的 34 个测试（当前总计 63 测试通过，含阶段 1+2 的 34）
- [x] `#![deny(missing_docs)]` 仍启用
- [x] spec 同步：ferrumdb-space 与 ferrumdb-btree 的 spec 更新到反映实际 API

## Constraints

- 引入 `tempfile` 用于集成测试创建临时表空间文件（workspace 依赖）
- 不引入 `unsafe`
- 不引入 `async`
- 不修改 ferrumdb-wal / ferrumdb-buffer / ferrumdb-txn（阶段 4/5/9 的事）
- 不修改 ferrumdb-engine 的 trait 签名
- 节点序列化用手写二进制（与 row 编码风格一致），不引入 serde

## Out of Scope

- WAL / 崩溃恢复（阶段 5）
- Buffer Pool / pin / LRU（阶段 4）
- 多表空间 / 数据库目录
- 并发访问
- 页压缩 / 加密
- ReadView / MVCC

## References

- `docs/plan.md` 阶段 3
- `docs/architecture.md` 磁盘布局
- `.trellis/spec/ferrumdb-space/backend/`
- `.trellis/spec/ferrumdb-btree/backend/`
- `.trellis/spec/ferrumdb-page/backend/database-guidelines.md`
- 阶段 2 lessons：`research/lessons.md`
