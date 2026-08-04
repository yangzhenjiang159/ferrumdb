//! `BufferPoolSource`：把 `BufferPool` 适配成 `PageSource`。

use ferrumdb_page::Page;
use ferrumdb_space::{PageSource, SpaceError};

use crate::error::BufferError;
use crate::pool::BufferPool;

/// 把 `BufferPool` 暴露为 `PageSource`，让 `PersistentBtree` 走 Buffer Pool。
///
/// # 类型
///
/// ```text
/// BufferPoolSource<'a>
///   ├── pool: &'a mut BufferPool
///   └── (impl PageSource)
/// ```
///
/// v1 简化：`read_page` 返回 owned `Page`（clone 自缓存）；`write_page` 取出
/// mutable guard 后覆盖 page 内容并标记 dirty。
pub struct BufferPoolSource<'a> {
    pool: &'a mut BufferPool,
}

impl<'a> BufferPoolSource<'a> {
    /// 从 `&mut BufferPool` 构造。
    pub fn new(pool: &'a mut BufferPool) -> Self {
        Self { pool }
    }
}

fn map_buffer_error(e: BufferError) -> SpaceError {
    match e {
        BufferError::Space(s) => s,
        BufferError::Page(_) => SpaceError::NotInitialized,
        BufferError::Io(_) => SpaceError::NotInitialized,
        BufferError::PoolFull => SpaceError::NotInitialized,
        BufferError::FrameNotFound(_) => SpaceError::NotInitialized,
        BufferError::PageNotInPool(_) => SpaceError::PageIdOutOfRange(0),
    }
}

impl<'a> PageSource for BufferPoolSource<'a> {
    fn read_page(&mut self, page_id: u32) -> Result<Page, SpaceError> {
        let guard = self.pool.fetch_page(page_id).map_err(map_buffer_error)?;
        Ok(guard.page().clone())
    }

    fn write_page(&mut self, page_id: u32, page: &Page) -> Result<(), SpaceError> {
        let mut guard = self.pool.fetch_page(page_id).map_err(map_buffer_error)?;
        *guard.page_mut() = page.clone();
        Ok(())
    }

    fn allocate_page(&mut self) -> Result<u32, SpaceError> {
        let guard = self.pool.allocate_page().map_err(map_buffer_error)?;
        Ok(guard.id())
    }
}

impl<'a> BufferPoolSource<'a> {
    /// 直接访问底层 `BufferPool`（供 `PersistentBtree` 等需要 `Space` 类似功能的调用方）。
    pub fn pool_mut(&mut self) -> &mut BufferPool {
        self.pool
    }
}
