# Design — 阶段 4 Buffer Pool

## 1. 模块依赖

```
ferrumdb-page ← ferrumdb-buffer ← (PersistentBtree + BufferPoolSource)
ferrumdb-space ←┘
```

`ferrumdb-buffer` 依赖 `ferrumdb-page` + `ferrumdb-space` + `thiserror`。

## 2. BufferPool 设计

### 2.1 结构

```rust
pub struct BufferPool {
    /// 底层 page 存储（Box<dyn PageSource>，允许 mock）
    source: Box<dyn PageSource>,
    /// Frame 表（最多 capacity 项）
    frames: Vec<Frame>,
    /// PageId → FrameId 映射
    table: HashMap<PageId, FrameId>,
    /// LRU 顺序（front = MRU, back = LRU）
    lru_order: Vec<FrameId>,
    /// 总容量（frame 数）
    capacity: usize,
}

pub struct Frame {
    page_id: Option<PageId>,   // None = free frame
    page: Page,
    pin_count: usize,
    is_dirty: bool,
}
```

### 2.2 构造函数

```rust
impl BufferPool {
    /// 从已有 tablespace 打开。
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self, BufferError> {
        let space = Space::open(path)?;
        Ok(Self::with_source(Box::new(space), capacity))
    }

    /// 创建新 tablespace。
    pub fn create(path: impl AsRef<Path>, capacity: usize) -> Result<Self, BufferError> {
        let space = Space::create(path)?;
        Ok(Self::with_source(Box::new(space), capacity))
    }

    /// 用任意 PageSource 构造（用于测试 + 灵活集成）。
    pub fn with_source(source: Box<dyn PageSource>, capacity: usize) -> Self {
        ...
    }
}
```

### 2.3 fetch_page 算法

```text
fn fetch_page(&mut self, page_id: PageId) -> Result<PageGuard, BufferError>:
    if page_id in self.table:
        frame_id = self.table[page_id]
        self.frames[frame_id].pin_count += 1
        self.touch_lru(frame_id)
        return Ok(PageGuard { pool: self, frame_id })

    # not in cache
    if self.frames.len() < self.capacity:
        frame_id = self.allocate_free_frame()  // push to frames
    else:
        frame_id = self.evict_or_fail()?       // may return PoolFull

    # load from disk
    self.frames[frame_id].page = self.source.read_page(page_id)?
    self.frames[frame_id].page_id = Some(page_id)
    self.frames[frame_id].pin_count = 1
    self.frames[frame_id].is_dirty = false
    self.table.insert(page_id, frame_id)
    self.touch_lru(frame_id)

    return Ok(PageGuard { pool: self, frame_id })
```

### 2.4 evict_or_fail

```text
fn evict_or_fail(&mut self) -> Result<FrameId, BufferError>:
    for frame_id in self.lru_order.iter().rev() {  // LRU first
        let f = &self.frames[*frame_id];
        if f.page_id.is_some() && f.pin_count == 0 && !f.is_dirty {
            // evict this frame
            let old_page_id = f.page_id.unwrap();
            self.table.remove(&old_page_id);
            // reuse frame
            self.frames[*frame_id].page_id = None;
            self.lru_order.retain(|id| id != frame_id);
            return Ok(*frame_id);
        }
        if f.page_id.is_some() && f.pin_count == 0 && f.is_dirty {
            // flush before evicting
            let pid = f.page_id.unwrap();
            let bytes = f.page.to_bytes();
            self.source.write_page(pid, &bytes)?;
            self.frames[*frame_id].is_dirty = false;
            // now eligible to evict
            ...
        }
    }
    Err(BufferError::PoolFull)
```

简化：第一次循环 flush dirty 页并标记；第二次循环 evict。

### 2.5 allocate_page

```text
fn allocate_page(&mut self) -> Result<PageGuard, BufferError>:
    let page_id = self.source.allocate_page()?;
    // Allocate a free frame and load the (just-allocated, zeroed) page into it.
    // We can either re-fetch from disk (read_page) or take the fresh Page from source.
    // Space::allocate_page returns a zero PageType::Free page; we keep that.
    let page = self.source.read_page(page_id)?;  // zeroed Free page
    let frame_id = ...;  // get a free frame (reuse from pool)
    self.frames[frame_id].page = page;
    self.frames[frame_id].page_id = Some(page_id);
    self.frames[frame_id].pin_count = 1;
    self.frames[frame_id].is_dirty = false;
    self.table.insert(page_id, frame_id);
    Ok(PageGuard { pool: self, frame_id })
```

### 2.6 flush_all

```text
fn flush_all(&mut self) -> Result<(), BufferError>:
    for frame in self.frames.iter_mut() {
        if frame.page_id.is_some() && frame.is_dirty {
            let pid = frame.page_id.unwrap();
            self.source.write_page(pid, &frame.page)?;
            frame.is_dirty = false;
        }
    }
    Ok(())
```

## 3. PageGuard 设计

```rust
pub struct PageGuard<'a> {
    pool: &'a mut BufferPool,
    frame_id: FrameId,
}

impl<'a> PageGuard<'a> {
    pub fn id(&self) -> PageId { self.pool.frames[self.frame_id].page_id.unwrap() }
    pub fn page(&self) -> &Page { &self.pool.frames[self.frame_id].page }
    pub fn page_mut(&mut self) -> &mut Page {
        let f = &mut self.pool.frames[self.frame_id];
        f.is_dirty = true;
        &mut f.page
    }
    pub fn mark_dirty(&mut self) {
        self.pool.frames[self.frame_id].is_dirty = true;
    }
}

impl<'a> Deref for PageGuard<'a> {
    type Target = Page;
    fn deref(&self) -> &Page { self.page() }
}

impl<'a> DerefMut for PageGuard<'a> {
    fn deref_mut(&mut self) -> &mut Page { self.page_mut() }
}

impl<'a> Drop for PageGuard<'a> {
    fn drop(&mut self) {
        // Decrement pin_count. Caller should have flushed if they cared.
        let f = &mut self.pool.frames[self.frame_id];
        f.pin_count = f.pin_count.saturating_sub(1);
    }
}
```

## 4. BufferPoolSource 适配器

```rust
pub struct BufferPoolSource<'a> {
    pool: &'a mut BufferPool,
}

impl<'a> PageSource for BufferPoolSource<'a> {
    fn read_page(&mut self, page_id: PageId) -> Result<Page, SpaceError> {
        let guard = self.pool.fetch_page(page_id).map_err(|e| match e {
            BufferError::Space(s) => s,
            other => SpaceError::FreeListCorrupted(0),  // map other variants
        })?;
        Ok(guard.page().clone())
    }
    fn write_page(&mut self, page_id: PageId, page: &Page) -> Result<(), SpaceError> {
        let mut guard = self.pool.fetch_page(page_id).map_err(|e| match e {
            BufferError::Space(s) => s,
            other => SpaceError::FreeListCorrupted(0),
        })?;
        *guard.page_mut() = page.clone();
        Ok(())
    }
    fn allocate_page(&mut self) -> Result<PageId, SpaceError> {
        let guard = self.pool.allocate_page().map_err(|e| match e {
            BufferError::Space(s) => s,
            other => SpaceError::FreeListCorrupted(0),
        })?;
        Ok(guard.id())
    }
}
```

注意：`read_page` 返回 `Page`（owned），但 `BufferPool` 持有原始 `Page`。每次 read 都 clone 是 v1 的简化（v2 可改为 return Arc<Page>）。

## 5. 锁顺序（文档）

v1 是单线程的 `&mut self`，所以没有真锁。但模块文档必须明确记录：

> **锁顺序（v1 单线程不适用，但 v2 多线程必须遵守）**：
> 1. 先获取 pool 锁
> 2. 再获取 frame-level 锁（如果实现）
>
> 反向顺序会死锁。

## 6. 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    #[error("buffer io: {0}")]
    Io(#[from] std::io::Error),

    #[error("page error: {0}")]
    Page(#[from] PageError),

    #[error("space error: {0}")]
    Space(#[from] SpaceError),

    #[error("pool full (all frames pinned or dirty)")]
    PoolFull,

    #[error("frame not found: {0}")]
    FrameNotFound(usize),
}
```

## 7. 文件改动清单

```
crates/ferrumdb-buffer/src/
├── lib.rs              (重导出 BufferPool, PageGuard, Frame, BufferError)
├── error.rs            (新增: BufferError)
├── frame.rs            (新增: Frame struct)
├── lru.rs              (新增: LRU 顺序 helpers)
├── pool.rs             (新增: BufferPool 主结构 + fetch/allocate/flush/evict)
├── guard.rs            (新增: PageGuard RAII)
└── source.rs           (新增: BufferPoolSource adapter 实现 PageSource)

crates/ferrumdb-btree/src/
└── persistent.rs       (可能需要小调整，让 PersistentBtree 通过 PageSource 工作 — 已经如此)
```

## 8. 风险与回滚

- **风险 A**：`PageGuard` 的 borrow 规则与 `&mut BufferPool` 冲突 — 通过把 `pool` 持有为 `&mut` 但只在内部方法里用解决
- **风险 B**：LRU 顺序 Vec 在大数据下移动 O(n) — v1 不优化，capacity 通常 < 10000
- **回滚**：BufferPool 与 PersistentBtree 通过 trait 解耦，删 BufferPool 不影响 B+Tree
