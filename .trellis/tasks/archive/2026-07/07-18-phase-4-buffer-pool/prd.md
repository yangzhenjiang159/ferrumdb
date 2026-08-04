# 阶段 4 — Buffer Pool

## Goal

按 `docs/plan.md` 阶段 4 要求，实现：

1. **`ferrumdb-buffer`**：内存 page 缓存
   - `BufferPool` 主结构
   - `Frame` (page + pin_count + dirty + lru_node)
   - `PageGuard<'a>` RAII pin holder（drop 时自动 unpin）
   - LRU 淘汰策略
   - 淘汰脏页前先 flush
   - 持锁顺序：先 pool 锁再 page 锁（仅 document，避免死锁）
2. **`BufferPoolSource`** 适配器：实现 `PageSource` trait，让 `PersistentBtree` 走 BufferPool
3. **集成测试**：通过 BufferPool 写入 1000 keys → flush → reopen → 验证

## Requirements

### R1 — BufferPool 主结构

| ID | 要求 |
|----|------|
| R1.1 | `BufferPool::open(path, capacity)` 从已有 tablespace 创建 + cache frames |
| R1.2 | `BufferPool::create(path, capacity)` 创建新 tablespace + cache frames |
| R1.3 | `fetch_page(page_id) -> Result<PageGuard, BufferError>` |
| R1.4 | `allocate_page() -> Result<PageGuard, BufferError>` |
| R1.5 | `flush_all() -> Result<(), BufferError>` 把所有 dirty frame 写回 Space |
| R1.6 | `flush_page(frame_id) -> Result<(), BufferError>` 单帧写回 |
| R1.7 | `capacity: usize` 固定容量，构造时设定 |
| R1.8 | 复用 ferrumdb-space 的 PageSource trait 抽象底层 |

### R2 — Frame 与 LRU

| ID | 要求 |
|----|------|
| R2.1 | `Frame { page_id: Option<u32>, page: Page, pin_count: usize, is_dirty: bool }` |
| R2.2 | `Vec<Frame>` 作为 frame table |
| R2.3 | `HashMap<PageId, FrameId>` 用于 O(1) 查找 |
| R2.4 | LRU 用 Vec<FrameId>（MRU 在前，LRU 在后） |
| R2.5 | `fetch_page` 命中时把 frame_id 移到 MRU 位置 |
| R2.6 | 淘汰候选 = pin_count == 0 && !is_dirty 的最久未用 frame |
| R2.7 | 全部 pinned 或 dirty → 返回 `BufferError::PoolFull` |
| R2.8 | 淘汰前先 flush dirty 页 |

### R3 — PageGuard RAII

| ID | 要求 |
|----|------|
| R3.1 | `PageGuard<'a>` 持有 `&'a mut BufferPool` + frame_id |
| R3.2 | `Deref<Target = Page>` + `DerefMut` |
| R3.3 | `page_mut()` 标记 dirty |
| R3.4 | `Drop` 时调用 `pool.unpin(frame_id)` |
| R3.5 | `id() -> PageId` |
| R3.6 | 不允许 `Clone`（pin 借用唯一） |
| R3.7 | 不暴露 `&mut BufferPool` 引用给用户（防止持锁逃逸） |

### R4 — BufferError

```rust
pub enum BufferError {
    Io(#[from] std::io::Error),
    Page(#[from] PageError),
    Space(#[from] SpaceError),
    PoolFull,             // 所有 frame 都 pinned 或 dirty
    FrameNotFound,        // 内部状态错误
}
```

### R5 — 与 PersistentBtree 集成

- 新增 `BufferPoolSource<'a>`：持有 `&'a mut BufferPool`，实现 `PageSource` trait
- `PersistentBtree` 不变；调用方把 `BufferPoolSource` 作为 source 参数
- 集成测试：1000 keys 插入走 BufferPoolSource → drop + flush → reopen 验证

## Acceptance Criteria

- [x] R1: open/create + fetch_page + allocate_page + flush_all 全部工作
- [x] R2: LRU 淘汰生效（mock Space 计数 disk read 验证）
- [x] R2: 淘汰脏页前先 flush
- [x] R3: PageGuard drop 触发 unpin（测试 pin_count 归零）
- [x] R3: page_mut 标记 dirty，drop 时不自动 flush（等下次 flush）
- [x] R4: 每个 BufferError 变体可达测试
- [x] R5: PersistentBtree 通过 BufferPoolSource 写入 1000 keys + reopen 全部可见
- [x] R5: BufferPoolSource 二次 fetch 同一 page_id 不触发 disk read（缓存命中）
- [x] `cargo build` 无 warning；`cargo test` 全过；`cargo clippy` 干净
- [x] 不破坏阶段 1+2+3 的 63 个测试
- [x] `#![deny(missing_docs)]` 仍启用
- [x] spec 同步：`ferrumdb-buffer/backend/` 6 文件全部填充

## Constraints

- 不引入新外部依赖
- 不引入 `unsafe`
- 不引入 `async`（Buffer Pool 是 sync）
- 不修改 ferrumdb-space / ferrumdb-btree 的核心 API
- PageGuard 不能跨 await 释放（无 await 所以不强制）
- 容量 >= 1

## Out of Scope

- 并发 Buffer Pool（v1 单线程）
- 多线程 pin 安全（v1 用 `Rc`/`RefCell` 而非 `Arc`/`Mutex`）
- 预读（read-ahead）
- 写合并（write coalescing）
- ARIES 风格的 steal/no-force 策略

## References

- `docs/plan.md` 阶段 4
- `docs/architecture.md` BTree → Buffer → Space
- `.trellis/spec/ferrumdb-buffer/backend/`（待填充）
- 阶段 3 `PageSource` trait 在 `crates/ferrumdb-space/src/page_source.rs`
