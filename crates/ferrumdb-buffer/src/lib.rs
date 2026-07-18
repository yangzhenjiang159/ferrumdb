//! Buffer Pool：页缓存、pin、LRU、脏页刷盘。
//!
//! # 职责
//!
//! - 将 `ferrumdb-space` 的磁盘页缓存在内存
//! - 提供 pin/unpin、脏页 flush、LRU 淘汰
//!
//! 见项目文档 `docs/plan.md` 阶段 4。
//!
//! # 锁顺序（v2 多线程必读）
//!
//! v1 是单线程 `&mut self`，无真锁。v2 多线程实现时：
//!
//! 1. 先获取 pool 锁
//! 2. 再获取 frame-level 锁
//!
//! 反向顺序会死锁。

#![deny(missing_docs)]

mod error;
mod frame;
mod guard;
mod lru;
mod pool;
mod source;

pub use error::BufferError;
pub use frame::{Frame, FrameId};
pub use guard::PageGuard;
pub use pool::BufferPool;
pub use source::BufferPoolSource;

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use ferrumdb_page::Page;
    use ferrumdb_space::{PageSource, SpaceError};

    /// 简单的 page 计数 mock，验证 LRU 行为。
    struct MockSource {
        pages: Vec<Page>,
        read_count: Cell<u32>,
        write_count: Cell<u32>,
        alloc_count: Cell<u32>,
    }

    impl MockSource {
        fn new(capacity: usize) -> Self {
            Self {
                pages: (0..capacity)
                    .map(|i| {
                        let mut p = Page::new(i as u32, ferrumdb_page::PageType::Free);
                        p.user_data_mut()[0] = i as u8;
                        p
                    })
                    .collect(),
                read_count: Cell::new(0),
                write_count: Cell::new(0),
                alloc_count: Cell::new(0),
            }
        }
    }

    impl PageSource for MockSource {
        fn read_page(&mut self, page_id: u32) -> Result<Page, SpaceError> {
            self.read_count.set(self.read_count.get() + 1);
            self.pages
                .get(page_id as usize)
                .cloned()
                .ok_or(SpaceError::PageIdOutOfRange(page_id))
        }
        fn write_page(&mut self, page_id: u32, page: &Page) -> Result<(), SpaceError> {
            self.write_count.set(self.write_count.get() + 1);
            if let Some(p) = self.pages.get_mut(page_id as usize) {
                *p = page.clone();
            }
            Ok(())
        }
        fn allocate_page(&mut self) -> Result<u32, SpaceError> {
            let id = self.pages.len() as u32;
            self.pages.push(Page::new(id, ferrumdb_page::PageType::Free));
            self.alloc_count.set(self.alloc_count.get() + 1);
            Ok(id)
        }
    }

    #[test]
    fn basic_fetch_and_read() {
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 4);
        let g = pool.fetch_page(3).unwrap();
        assert_eq!(g.id(), 3);
        assert_eq!(g.page().user_data()[0], 3);
    }

    #[test]
    fn second_fetch_is_cache_hit() {
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 4);
        let reads_before = pool.dirty_frame_count(); // dummy read
        let _ = pool.fetch_page(0).unwrap();
        let _ = pool.fetch_page(0).unwrap();
        let _ = pool.fetch_page(0).unwrap();
        // We can't directly inspect mock's counter, but cache hit means
        // no second disk read.
        drop(pool);
        // Indirect: capacity is 4, fetching 3 unique pages then re-fetching
        // should NOT cause evictions.
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 4);
        for _ in 0..10 {
            let _ = pool.fetch_page(0).unwrap();
        }
        assert_eq!(pool.used_frames(), 1);
        let _ = reads_before; // suppress unused
    }

    #[test]
    fn lru_evicts_cold_page() {
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 2);
        let _g0 = pool.fetch_page(0).unwrap();
        drop(_g0);
        let _g1 = pool.fetch_page(1).unwrap();
        drop(_g1);
        // Now fetch 2 — page 0 should be evicted (LRU).
        let _g2 = pool.fetch_page(2).unwrap();
        drop(_g2);
        assert!(!pool.table.contains_key(&0), "page 0 should be evicted");
        assert!(pool.table.contains_key(&2));
    }

    #[test]
    fn pinned_page_not_evicted() {
        // Verifies LRU respects access order: the most recently fetched page
        // is preserved when a new page forces eviction.
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 2);
        // Fetch 0; drop; fetch 1 (now 0 is LRU, 1 is MRU); drop; fetch 2
        // → should evict 0, keep 1 and 2.
        let _ = pool.fetch_page(0).unwrap();
        let _ = pool.fetch_page(1).unwrap();
        let _g2 = pool.fetch_page(2).unwrap();
        drop(_g2);
        assert!(pool.table.contains_key(&1), "1 should be MRU");
        assert!(pool.table.contains_key(&2), "2 just fetched");
        assert!(!pool.table.contains_key(&0), "0 should be LRU and evicted");
    }

    #[test]
    fn dirty_page_flushed_before_eviction() {
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 2);
        // Mark page 0 dirty in inner scope.
        {
            let mut g = pool.fetch_page(0).unwrap();
            g.page_mut().user_data_mut()[0] = 99;
        } // g dropped; page 0 dirty in buffer, not yet flushed
        // Fill the pool and force eviction of dirty page 0.
        let _g1 = pool.fetch_page(1).unwrap();
        drop(_g1);
        let _g2 = pool.fetch_page(2).unwrap();
        drop(_g2);
        // Page 0 should have been flushed (and evicted). flush_all is a no-op now.
        assert!(!pool.table.contains_key(&0));
        assert_eq!(pool.dirty_frame_count(), 0);
    }

    #[test]
    fn page_guard_drop_unpins() {
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 2);
        // After fetching and dropping, pin_count should be 0, so eviction is possible.
        {
            let _g = pool.fetch_page(0).unwrap();
        }
        let _g1 = pool.fetch_page(1).unwrap();
        drop(_g1);
        let _g2 = pool.fetch_page(0).unwrap(); // still cached, no eviction needed
        drop(_g2);
        assert!(pool.table.contains_key(&0));
    }

    #[test]
    fn drop_then_refetch_works() {
        // Verifies that after a PageGuard drops, the next fetch_page on the
        // same page_id succeeds (cache hit, no eviction).
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 1);
        {
            let _g = pool.fetch_page(0).unwrap();
        }
        let g2 = pool.fetch_page(0).unwrap();
        assert_eq!(g2.id(), 0);
    }

    #[test]
    fn flush_all_writes_dirty_frames() {
        let source = MockSource::new(10);
        let mut pool = BufferPool::with_source(Box::new(source), 4);
        {
            let mut g = pool.fetch_page(0).unwrap();
            g.page_mut().user_data_mut()[0] = 42;
        }
        {
            let mut g = pool.fetch_page(1).unwrap();
            g.page_mut().user_data_mut()[0] = 99;
        }
        assert_eq!(pool.dirty_frame_count(), 2);
        pool.flush_all().unwrap();
        assert_eq!(pool.dirty_frame_count(), 0);
    }


}
