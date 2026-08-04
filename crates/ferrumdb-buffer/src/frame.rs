//! Frame：Buffer Pool 中缓存的单个页。

use ferrumdb_page::Page;

/// Frame 索引（在 `BufferPool.frames` 中的位置）。
pub type FrameId = usize;

/// Buffer Pool 中缓存的页及其状态。
#[derive(Debug, Clone)]
pub struct Frame {
    /// 关联的 `PageId`；`None` 表示该 frame 是 free（未关联任何 page）。
    pub page_id: Option<u32>,
    /// 缓存的页内容。
    pub page: Page,
    /// pin 计数；> 0 表示被某个 PageGuard 持有，禁止淘汰。
    pub pin_count: usize,
    /// 是否 dirty；true 表示需要 flush 到底层存储。
    pub is_dirty: bool,
}

impl Frame {
    /// 创建一个空的 free frame。
    pub fn free() -> Self {
        // Free frame holds a placeholder Page; page_id is None.
        Self {
            page_id: None,
            page: Page::new(0, ferrumdb_page::PageType::Free),
            pin_count: 0,
            is_dirty: false,
        }
    }

    /// 该 frame 是否可淘汰？
    pub fn is_evictable(&self) -> bool {
        self.page_id.is_some() && self.pin_count == 0 && !self.is_dirty
    }
}
