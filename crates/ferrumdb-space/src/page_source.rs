//! `PageSource` trait：把 B+Tree 与底层 page 存储解耦。

use ferrumdb_page::Page;

use crate::error::SpaceError;

/// 抽象的页存储：B+Tree 通过该 trait 读写页、分配新页。
///
/// 由 `Space` 实现（生产路径），测试可注入 mock 实现。
pub trait PageSource {
    /// 读取 `page_id` 对应的页。
    fn read_page(&mut self, page_id: u32) -> Result<Page, SpaceError>;

    /// 写入 `page_id` 对应的页。
    fn write_page(&mut self, page_id: u32, page: &Page) -> Result<(), SpaceError>;

    /// 分配一个新页并返回其 id。新页内容未初始化。
    fn allocate_page(&mut self) -> Result<u32, SpaceError>;
}

impl<T: PageSource + ?Sized> PageSource for &mut T {
    fn read_page(&mut self, page_id: u32) -> Result<Page, SpaceError> {
        (**self).read_page(page_id)
    }
    fn write_page(&mut self, page_id: u32, page: &Page) -> Result<(), SpaceError> {
        (**self).write_page(page_id, page)
    }
    fn allocate_page(&mut self) -> Result<u32, SpaceError> {
        (**self).allocate_page()
    }
}

impl<T: PageSource + ?Sized> PageSource for Box<T> {
    fn read_page(&mut self, page_id: u32) -> Result<Page, SpaceError> {
        (**self).read_page(page_id)
    }
    fn write_page(&mut self, page_id: u32, page: &Page) -> Result<(), SpaceError> {
        (**self).write_page(page_id, page)
    }
    fn allocate_page(&mut self) -> Result<u32, SpaceError> {
        (**self).allocate_page()
    }
}

impl PageSource for crate::space::Space {
    fn read_page(&mut self, page_id: u32) -> Result<ferrumdb_page::Page, SpaceError> {
        self.read_page(page_id)
    }
    fn write_page(&mut self, page_id: u32, page: &ferrumdb_page::Page) -> Result<(), SpaceError> {
        self.write_page(page_id, page)
    }
    fn allocate_page(&mut self) -> Result<u32, SpaceError> {
        self.allocate_page()
    }
}
