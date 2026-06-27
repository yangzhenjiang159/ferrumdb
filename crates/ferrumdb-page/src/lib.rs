//! 16KB 页格式、页头与行编解码。
//!
//! # 职责
//!
//! - 固定大小页（[`PAGE_SIZE`] = 16384）的序列化与校验
//! - Slotted Page 行布局（阶段 2）
//! - [`Row`] / [`Schema`] / [`Value`] 类型（阶段 1–2 完善）
//!
//! 见项目文档 `docs/plan.md` 阶段 1。

mod error;
mod page;
mod row;

pub use error::PageError;
pub use page::{
    Page, PageFooter, PageHeader, PageType, PAGE_FOOTER_OFFSET, PAGE_FOOTER_SIZE,
    PAGE_HEADER_SIZE, PAGE_MAGIC, PAGE_HEADER_VERSION, PAGE_SIZE, PAGE_USER_DATA_OFFSET,
    PAGE_USER_DATA_SIZE,
};
pub use row::{Row, Schema, Value};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_and_schema_are_constructible() {
        let row = Row {
            values: vec![Value::I64(1), Value::Bytes(b"ferrumdb".to_vec())],
        };
        let schema = Schema {
            columns: vec!["id".into(), "name".into()],
        };
        assert_eq!(row.values.len(), schema.columns.len());
    }
}
