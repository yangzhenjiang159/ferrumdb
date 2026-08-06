//! 16KB 页格式、页头与行编解码。
//!
//! # 职责
//!
//! - 固定大小页（[`PAGE_SIZE`] = 16384）的序列化与校验
//! - [`Row`] / [`Schema`] / [`Value`] / [`ColumnType`] 类型与编解码
//! - [`SlottedPage`] 行槽位布局
//! - [`key`] B+Tree 键的保序编码（阶段 6）
//!
//! 见项目文档 `docs/plan.md` 阶段 1–2、阶段 6。

mod error;
mod key;
mod page;
mod row;
mod slotted;

pub use error::PageError;
pub use key::{
    decode_key, encode_index_key, encode_key, encode_pk, encode_secondary_key,
    primary_key_type, successor, upper_bound, UPPER_BOUND_BYTES_LEN,
};
pub use page::{
    Page, PageFooter, PageHeader, PageType, PAGE_FOOTER_OFFSET, PAGE_FOOTER_SIZE,
    PAGE_HEADER_SIZE, PAGE_MAGIC, PAGE_HEADER_VERSION, PAGE_SIZE, PAGE_USER_DATA_OFFSET,
    PAGE_USER_DATA_SIZE,
};
pub use row::{decode_row, encode_row, ColumnType, Row, Schema, Value};
pub use slotted::{SlotEntry, SlottedPage};

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
            types: vec![ColumnType::I64, ColumnType::Bytes],
            primary_key: Some(0),
        };
        assert_eq!(row.values.len(), schema.columns.len());
    }
}
