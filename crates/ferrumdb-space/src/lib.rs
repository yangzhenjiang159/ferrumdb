//! 表空间文件管理与页分配。
//!
//! # 职责
//!
//! - 表空间文件布局（superblock、页号 → 文件偏移）
//! - 空闲页分配与回收
//! - `PageSource` trait 抽象，让 B+Tree 不直接依赖文件 I/O
//!
//! 见项目文档 `docs/plan.md` 阶段 3。

#![deny(missing_docs)]

mod error;
mod free_list;
mod page_source;
mod space;
mod superblock;

pub use error::SpaceError;
pub use page_source::PageSource;
pub use space::Space;
pub use superblock::Superblock;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
