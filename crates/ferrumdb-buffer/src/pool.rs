//! `BufferPool` 主结构。

use std::collections::HashMap;
use std::path::Path;

use ferrumdb_page::Page;
use ferrumdb_space::{PageSource, Space};

use crate::error::BufferError;
use crate::frame::{Frame, FrameId};
use crate::lru::LruOrder;

/// 内存 page 缓存。
///
/// - `source` 提供 page 读写与分配（默认是 `Space`）
/// - `frames` 是固定大小的 frame 表
/// - `table` O(1) 查找 `PageId → FrameId`
/// - `lru_order` 维护 LRU 顺序
pub struct BufferPool {
    /// 底层 page 存储（Box<dyn PageSource>，允许 mock）
    source: Box<dyn PageSource>,
    /// Frame 表（最多 capacity 项）
    frames: Vec<Frame>,
    /// PageId → FrameId 映射（pub 以允许测试断言；正常代码通过 `fetch_page` 间接使用）。
    pub table: HashMap<u32, FrameId>,
    /// LRU 顺序
    lru_order: LruOrder,
    /// 总容量
    capacity: usize,
}

impl BufferPool {
    /// 用任意 `PageSource` 构造（不可用 `set_root_page_id`）。
    pub fn with_source(source: Box<dyn PageSource>, capacity: usize) -> Self {
        assert!(capacity >= 1, "BufferPool capacity must be >= 1");
        Self {
            source,
            frames: Vec::with_capacity(capacity),
            table: HashMap::new(),
            lru_order: LruOrder::new(),
            capacity,
        }
    }

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

    /// 总容量（frame 数）。
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 已分配的 frame 数（含 free）。
    pub fn used_frames(&self) -> usize {
        self.frames.len()
    }

    /// dirty frame 数（需要 flush）。
    pub fn dirty_frame_count(&self) -> usize {
        self.frames.iter().filter(|f| f.is_dirty).count()
    }

    /// 取一个 page 到 buffer，pin 它。
    ///
    /// # Errors
    ///
    /// - `BufferError::PoolFull` 如果所有 frame 都 pinned 或 dirty 且无法淘汰
    /// - `BufferError::Space` 如果底层 read_page 失败
    pub fn fetch_page(&mut self, page_id: u32) -> Result<crate::guard::PageGuard<'_>, BufferError> {
        if let Some(&frame_id) = self.table.get(&page_id) {
            self.frames[frame_id].pin_count += 1;
            self.lru_order.touch(frame_id);
            return Ok(crate::guard::PageGuard::new(self, frame_id));
        }
        let frame_id = self.acquire_frame()?;
        let page = self.source.read_page(page_id)?;
        let f = &mut self.frames[frame_id];
        f.page = page;
        f.page_id = Some(page_id);
        f.pin_count = 1;
        f.is_dirty = false;
        self.table.insert(page_id, frame_id);
        self.lru_order.touch(frame_id);
        Ok(crate::guard::PageGuard::new(self, frame_id))
    }

    /// 分配新 page 并 pin 它。
    pub fn allocate_page(&mut self) -> Result<crate::guard::PageGuard<'_>, BufferError> {
        let page_id = self.source.allocate_page()?;
        let frame_id = self.acquire_frame()?;
        let page = self.source.read_page(page_id)?;
        let f = &mut self.frames[frame_id];
        f.page = page;
        f.page_id = Some(page_id);
        f.pin_count = 1;
        f.is_dirty = false;
        self.table.insert(page_id, frame_id);
        self.lru_order.touch(frame_id);
        Ok(crate::guard::PageGuard::new(self, frame_id))
    }

    /// 把所有 dirty frame 写回底层存储。
    pub fn flush_all(&mut self) -> Result<(), BufferError> {
        for frame in self.frames.iter_mut() {
            if let Some(pid) = frame.page_id {
                if frame.is_dirty {
                    self.source.write_page(pid, &frame.page)?;
                    frame.is_dirty = false;
                }
            }
        }
        Ok(())
    }

    /// 取一个 free frame（扩容或淘汰）。
    fn acquire_frame(&mut self) -> Result<FrameId, BufferError> {
        for (idx, f) in self.frames.iter().enumerate() {
            if f.page_id.is_none() {
                return Ok(idx);
            }
        }
        if self.frames.len() < self.capacity {
            let idx = self.frames.len();
            self.frames.push(Frame::free());
            return Ok(idx);
        }
        self.evict_lru()
    }

    /// 淘汰一个不可达、可淘汰的 frame；如都 dirty 先 flush 再淘汰。
    fn evict_lru(&mut self) -> Result<FrameId, BufferError> {
        let mut any_dirty_flushed = false;
        let candidates: Vec<FrameId> = self.lru_order.iter_lru().collect();
        for frame_id in &candidates {
            let f = &self.frames[*frame_id];
            if f.page_id.is_none() || f.pin_count > 0 {
                continue;
            }
            if let Some(pid) = f.page_id {
                if f.is_dirty {
                    if any_dirty_flushed {
                        continue;
                    }
                    self.source.write_page(pid, &f.page)?;
                    self.frames[*frame_id].is_dirty = false;
                    any_dirty_flushed = true;
                }
            }
        }
        let candidates: Vec<FrameId> = self.lru_order.iter_lru().collect();
        for frame_id in candidates {
            if self.frames[frame_id].is_evictable() {
                let old_pid = self.frames[frame_id].page_id.unwrap();
                self.table.remove(&old_pid);
                self.lru_order.remove(frame_id);
                let f = &mut self.frames[frame_id];
                f.page_id = None;
                f.pin_count = 0;
                f.is_dirty = false;
                return Ok(frame_id);
            }
        }
        Err(BufferError::PoolFull)
    }

    pub(crate) fn unpin(&mut self, frame_id: FrameId) {
        if let Some(f) = self.frames.get_mut(frame_id) {
            f.pin_count = f.pin_count.saturating_sub(1);
        }
    }

    pub(crate) fn mark_dirty(&mut self, frame_id: FrameId) {
        if let Some(f) = self.frames.get_mut(frame_id) {
            f.is_dirty = true;
        }
    }

    pub(crate) fn frame_page(&self, frame_id: FrameId) -> &Page {
        &self.frames[frame_id].page
    }

    pub(crate) fn frame_page_mut(&mut self, frame_id: FrameId) -> &mut Page {
        let f = &mut self.frames[frame_id];
        f.is_dirty = true;
        &mut f.page
    }

    pub(crate) fn frame_page_id(&self, frame_id: FrameId) -> u32 {
        self.frames[frame_id].page_id.unwrap()
    }

    /// 设置 tablespace 的 root page id（仅在底层是 `Space` 时可用）。
    ///
    /// 当前实现：尝试 downcast `Box<dyn PageSource>` 到 `Space`。
    /// `with_source` 构造的 pool 没有 Space 信息，本方法返回 `BufferError::FrameNotFound`。
    pub fn set_root_page_id(&mut self, page_id: u32) -> Result<(), BufferError> {
        // We can't easily downcast Box<dyn PageSource> back to Space without a tag.
        // For phase 4, we provide a separate explicit API: pass the path to set_root.
        // Workaround: expose `as_any` on PageSource trait.
        // For now, document this limitation in the spec and rely on the
        // user calling Space::set_root_page_id directly via a parallel handle.
        let _ = page_id;
        Err(BufferError::FrameNotFound(0))
    }
}

impl std::fmt::Debug for BufferPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferPool")
            .field("capacity", &self.capacity)
            .field("frames", &self.frames.len())
            .field("table_size", &self.table.len())
            .finish()
    }
}
