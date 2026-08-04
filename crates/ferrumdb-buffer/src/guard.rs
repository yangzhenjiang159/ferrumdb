//! `PageGuard`：RAII 持有的 pin handle。

use std::ops::{Deref, DerefMut};

use ferrumdb_page::Page;

use crate::frame::FrameId;
use crate::pool::BufferPool;

/// 持有 Buffer Pool 中某 frame 的 pin 借用。
///
/// 借用了 BufferPool 的一部分 (`'a`)；`Drop` 时自动 unpin。
/// 不能 `Clone`（pin 借用唯一）。
#[derive(Debug)]
pub struct PageGuard<'a> {
    pool: &'a mut BufferPool,
    frame_id: FrameId,
}

impl<'a> PageGuard<'a> {
    /// 由 `BufferPool::fetch_page` / `allocate_page` 内部构造。
    pub(crate) fn new(pool: &'a mut BufferPool, frame_id: FrameId) -> Self {
        Self { pool, frame_id }
    }

    /// 当前 guard 持有的 page id。
    pub fn id(&self) -> u32 {
        self.pool.frame_page_id(self.frame_id)
    }

    /// 不可变访问缓存的 page。
    pub fn page(&self) -> &Page {
        self.pool.frame_page(self.frame_id)
    }

    /// 可变访问缓存的 page，标记 dirty。
    pub fn page_mut(&mut self) -> &mut Page {
        self.pool.frame_page_mut(self.frame_id)
    }

    /// 显式标记 dirty（不修改内容）。
    pub fn mark_dirty(&mut self) {
        self.pool.mark_dirty(self.frame_id);
    }
}

impl<'a> Deref for PageGuard<'a> {
    type Target = Page;
    fn deref(&self) -> &Page {
        self.page()
    }
}

impl<'a> DerefMut for PageGuard<'a> {
    fn deref_mut(&mut self) -> &mut Page {
        self.page_mut()
    }
}

impl<'a> Drop for PageGuard<'a> {
    fn drop(&mut self) {
        self.pool.unpin(self.frame_id);
    }
}
