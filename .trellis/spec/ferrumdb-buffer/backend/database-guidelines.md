# Database Guidelines — `ferrumdb-buffer`

Buffer Pool 在内存中缓存 page，并提供 LRU 淘汰与 dirty flush。

---

## Frame 表

```rust
pub struct Frame {
    pub page_id: Option<u32>,  // None = free frame
    pub page: Page,
    pub pin_count: usize,      // > 0 = 不能淘汰
    pub is_dirty: bool,        // true = 需要 flush
}
```

- `frames: Vec<Frame>`，最多 `capacity` 个
- `table: HashMap<PageId, FrameId>` 用于 O(1) 查找
- `lru_order: Vec<FrameId>` MRU 在前，LRU 在后

## Pin 协议

```rust
let guard = pool.fetch_page(0)?;   // pin_count = 1
let page: &Page = guard.page();    // 不可变访问
// drop(guard) → pin_count = 0
```

- `pin_count` 由 `fetch_page` / `allocate_page` 自增
- `PageGuard::drop` 自减（saturating_sub）
- 显式 API：无 `pool.unpin(frame_id)` 公开方法

## LRU 淘汰

`evict_lru` 算法（按顺序）：

1. 遍历 `lru_order`（从 LRU 端开始）
2. 跳过 `page_id.is_none()` 或 `pin_count > 0` 的 frame
3. 找到首个 `!is_dirty` 且可淘汰的 frame → 移除
4. 如果没有 clean frame，flush 一个 dirty frame 后再找
5. 全部 pinned 或 dirty → 返回 `BufferError::PoolFull`

`acquire_frame` 流程：
- 优先复用 `page_id.is_none()` 的 free frame
- 否则若 `frames.len() < capacity`，push 新 free frame
- 否则调用 `evict_lru`

## Dirty Flush

- `PageGuard::page_mut` 自动 mark dirty
- `PageGuard::mark_dirty` 显式 mark dirty
- `BufferPool::flush_all` 写所有 dirty frame
- 淘汰 dirty frame 前**先 flush**（`evict_lru` 第 4 步）

## 锁顺序（v1 单线程不强制，v2 多线程必读）

1. 先获取 pool 锁
2. 再获取 frame-level 锁（如有）

**反向顺序会死锁**。模块文档已记录此约束。

## 与 PageSource 的关系

`BufferPool` 不直接依赖 `Space`：

```rust
pub fn open(path: P, capacity: usize) -> Result<Self, BufferError> {
    let space = Space::open(path)?;
    Ok(Self::with_source(Box::new(space), capacity))
}
```

- `with_source` 接受任意 `Box<dyn PageSource>`（用于测试）
- `BufferPoolSource<'a>` 是反向适配器：把 `BufferPool` 暴露为 `PageSource`
- `PersistentBtree` 通过 `BufferPoolSource` 间接走 Buffer Pool

## API Summary

```rust
impl BufferPool {
    pub fn with_source(source: Box<dyn PageSource>, capacity: usize) -> Self;
    pub fn open(path: P, capacity: usize) -> Result<Self, BufferError>;
    pub fn create(path: P, capacity: usize) -> Result<Self, BufferError>;
    pub fn fetch_page(&mut self, page_id: u32) -> Result<PageGuard<'_>, BufferError>;
    pub fn allocate_page(&mut self) -> Result<PageGuard<'_>, BufferError>;
    pub fn flush_all(&mut self) -> Result<(), BufferError>;
    pub fn set_root_page_id(&mut self, page_id: u32) -> Result<(), BufferError>;  // placeholder
    pub fn capacity(&self) -> usize;
    pub fn used_frames(&self) -> usize;
    pub fn dirty_frame_count(&self) -> usize;
}

impl<'a> PageGuard<'a> {
    pub fn id(&self) -> u32;
    pub fn page(&self) -> &Page;
    pub fn page_mut(&mut self) -> &mut Page;
    pub fn mark_dirty(&mut self);
}

impl<'a> BufferPoolSource<'a> {
    pub fn new(pool: &'a mut BufferPool) -> Self;
    pub fn pool_mut(&mut self) -> &mut BufferPool;
}
```

## Anti-Patterns

- ❌ 持有 `PageGuard` 时调用 `pool.fetch_page`（借用冲突）
- ❌ 修改 `BufferPool.table` 字段（保留 `pub` 是为了测试，正常代码应通过 `fetch_page` 间接使用）
- ❌ 持有 `&mut BufferPool` 跨 await point（v1 是 sync 库）
- ❌ 假设 `set_root_page_id` 工作（当前是 placeholder；通过 `Space` 直接设置 superblock.root_page_id）
- ❌ 写入 dirty frame 后不调用 `flush_all`（数据可能丢失）
- ❌ 淘汰 dirty frame 时不 flush（违反 "dirty before evict" 铁律）
