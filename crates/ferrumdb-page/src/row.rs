//! 行、列类型、表结构与编解码（阶段 2 完善）。
//!
//! # 编码格式
//!
//! 全 little-endian（项目约定，见 `docs/plan.md` 阶段 1 字节序说明）。
//!
//! ```text
//! +----------------------------+
//! | null_bitmap (N bytes)      |  N = ceil(column_count / 8); bit i set => column i is NULL
//! +----------------------------+
//! | column 0 bytes             |
//! | column 1 bytes             |
//! | ...                        |
//! | column (n-1) bytes         |
//! +----------------------------+
//! ```
//!
//! 每列的字节布局取决于 [`Value`] 变体：
//!
//! | 变体 | 字节 |
//! |------|------|
//! | `Null` | 0（bitmap 标记） |
//! | `I64(i64)` | 8 字节 LE |
//! | `Bytes(Vec<u8>)` | `[len: u32 LE (4B)][bytes]` |
//!
//! 解码需要 [`Schema`] 提供列类型，因此编码与解码必须成对使用同一份 schema。

use crate::error::PageError;

/// 单元格值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// SQL NULL
    Null,
    /// 有符号 64 位整数
    I64(i64),
    /// 变长字节（字符串、二进制等）
    Bytes(Vec<u8>),
}

/// 列类型（阶段 2 引入；阶段 1 stub 不带类型信息）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    /// 定长 8 字节有符号整数。
    I64,
    /// 变长字节串。
    Bytes,
    /// 类型由运行时推断（不参与编码，仅用于调试/未来扩展）。
    Any,
}

/// 一行数据，列顺序与 [`Schema::columns`] 一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub values: Vec<Value>,
}

/// 表结构描述。
///
/// 阶段 1 只有 `columns`；阶段 2 扩展了类型与主键信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    /// 列名（顺序与 `Row::values` 一致）。
    pub columns: Vec<String>,
    /// 每列的类型（顺序与 `columns` 一致）。
    pub types: Vec<ColumnType>,
    /// 主键列索引（`None` 表示无主键）。
    pub primary_key: Option<usize>,
}

impl Schema {
    /// 仅含列名的快速构造函数（不指定类型）。仅用于测试；生产路径必须填 `types`。
    pub fn from_names(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let columns: Vec<String> = names.into_iter().map(Into::into).collect();
        let types = vec![ColumnType::Any; columns.len()];
        Self {
            columns,
            types,
            primary_key: None,
        }
    }
}

fn bitmap_len(column_count: usize) -> usize {
    column_count.div_ceil(8)
}

/// 将一行按 schema 编码为字节序列。
///
/// # Errors
///
/// - `Row::values.len()` 与 `Schema::columns.len()` 不一致 ⇒ `PageError::EncodingError`
pub fn encode_row(row: &Row, schema: &Schema) -> Result<Vec<u8>, PageError> {
    if row.values.len() != schema.columns.len() {
        return Err(PageError::EncodingError(format!(
            "row has {} values but schema expects {}",
            row.values.len(),
            schema.columns.len()
        )));
    }
    let n = schema.columns.len();
    let bm_len = bitmap_len(n);
    let mut bitmap = vec![0u8; bm_len];
    let mut payload = Vec::new();

    for (i, value) in row.values.iter().enumerate() {
        match value {
            Value::Null => {
                bitmap[i / 8] |= 1 << (i % 8);
            }
            Value::I64(n_) => {
                if schema.types[i] == ColumnType::Bytes {
                    return Err(PageError::EncodingError(format!(
                        "column {i}: schema expects Bytes, got I64"
                    )));
                }
                payload.extend_from_slice(&n_.to_le_bytes());
            }
            Value::Bytes(b) => {
                if schema.types[i] == ColumnType::I64 {
                    return Err(PageError::EncodingError(format!(
                        "column {i}: schema expects I64, got Bytes"
                    )));
                }
                if b.len() > u32::MAX as usize {
                    return Err(PageError::EncodingError(format!(
                        "column {i}: bytes length {} exceeds u32::MAX",
                        b.len()
                    )));
                }
                payload.extend_from_slice(&(b.len() as u32).to_le_bytes());
                payload.extend_from_slice(b);
            }
        }
    }

    let mut out = Vec::with_capacity(bm_len + payload.len());
    out.extend_from_slice(&bitmap);
    out.extend_from_slice(&payload);
    Ok(out)
}

/// 将字节序列按 schema 解码为一行。
///
/// # Errors
///
/// - 字节长度小于 bitmap 最小长度
/// - `Bytes` 字段声明的长度超出剩余字节
/// - 列类型与值不匹配
pub fn decode_row(bytes: &[u8], schema: &Schema) -> Result<Row, PageError> {
    let n = schema.columns.len();
    let bm_len = bitmap_len(n);
    if bytes.len() < bm_len {
        return Err(PageError::EncodingError(format!(
            "row bytes too short: {} < bitmap length {}",
            bytes.len(),
            bm_len
        )));
    }
    let bitmap = &bytes[..bm_len];
    let mut rest = &bytes[bm_len..];
    let mut values = Vec::with_capacity(n);

    for i in 0..n {
        if bitmap[i / 8] & (1 << (i % 8)) != 0 {
            values.push(Value::Null);
            continue;
        }
        match schema.types[i] {
            ColumnType::I64 => {
                if rest.len() < 8 {
                    return Err(PageError::EncodingError(format!(
                        "column {i}: need 8 bytes for I64, only {} left",
                        rest.len()
                    )));
                }
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&rest[..8]);
                rest = &rest[8..];
                values.push(Value::I64(i64::from_le_bytes(buf)));
            }
            ColumnType::Bytes => {
                if rest.len() < 4 {
                    return Err(PageError::EncodingError(format!(
                        "column {i}: need 4 bytes for length prefix, only {} left",
                        rest.len()
                    )));
                }
                let mut len_buf = [0u8; 4];
                len_buf.copy_from_slice(&rest[..4]);
                rest = &rest[4..];
                let len = u32::from_le_bytes(len_buf) as usize;
                if rest.len() < len {
                    return Err(PageError::EncodingError(format!(
                        "column {i}: declared {} bytes, only {} left",
                        len,
                        rest.len()
                    )));
                }
                values.push(Value::Bytes(rest[..len].to_vec()));
                rest = &rest[len..];
            }
            ColumnType::Any => {
                return Err(PageError::EncodingError(format!(
                    "column {i}: schema has ColumnType::Any, cannot decode"
                )));
            }
        }
    }
    Ok(Row { values })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_i64_and_bytes() -> Schema {
        Schema {
            columns: vec!["id".into(), "name".into()],
            types: vec![ColumnType::I64, ColumnType::Bytes],
            primary_key: Some(0),
        }
    }

    #[test]
    fn round_trip_mixed_columns() {
        let schema = schema_i64_and_bytes();
        let row = Row {
            values: vec![Value::I64(42), Value::Bytes(b"ferrumdb".to_vec())],
        };
        let bytes = encode_row(&row, &schema).unwrap();
        let decoded = decode_row(&bytes, &schema).unwrap();
        assert_eq!(row, decoded);
    }

    #[test]
    fn null_bitmap_round_trip() {
        let schema = Schema {
            columns: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
            types: vec![ColumnType::I64; 5],
            primary_key: None,
        };
        let row = Row {
            values: vec![
                Value::Null,
                Value::I64(7),
                Value::Null,
                Value::I64(-1),
                Value::Null,
            ],
        };
        let bytes = encode_row(&row, &schema).unwrap();
        let decoded = decode_row(&bytes, &schema).unwrap();
        assert_eq!(row, decoded);
    }

    #[test]
    fn i64_little_endian() {
        let schema = Schema {
            columns: vec!["x".into()],
            types: vec![ColumnType::I64],
            primary_key: None,
        };
        let row = Row {
            values: vec![Value::I64(i64::MIN)],
        };
        let bytes = encode_row(&row, &schema).unwrap();
        // Bitmap is 1 byte; bit 0 = 0 => NOT NULL (correct for I64).
        assert_eq!(&bytes[..1], &[0]);
        assert_eq!(&bytes[1..], &i64::MIN.to_le_bytes());
    }

    #[test]
    fn bytes_length_prefix_round_trip() {
        let schema = Schema {
            columns: vec!["s".into()],
            types: vec![ColumnType::Bytes],
            primary_key: None,
        };
        let payload = vec![0u8, 1, 2, 3, 255];
        let row = Row {
            values: vec![Value::Bytes(payload.clone())],
        };
        let bytes = encode_row(&row, &schema).unwrap();
        let decoded = decode_row(&bytes, &schema).unwrap();
        assert_eq!(decoded.values[0], Value::Bytes(payload));
    }

    #[test]
    fn arity_mismatch_returns_error() {
        let schema = Schema {
            columns: vec!["a".into(), "b".into()],
            types: vec![ColumnType::I64, ColumnType::I64],
            primary_key: None,
        };
        let row = Row {
            values: vec![Value::I64(1)],
        };
        assert!(matches!(
            encode_row(&row, &schema),
            Err(PageError::EncodingError(_))
        ));
    }

    #[test]
    fn type_mismatch_returns_error() {
        let schema = Schema {
            columns: vec!["x".into()],
            types: vec![ColumnType::I64],
            primary_key: None,
        };
        let row = Row {
            values: vec![Value::Bytes(b"nope".to_vec())],
        };
        assert!(matches!(
            encode_row(&row, &schema),
            Err(PageError::EncodingError(_))
        ));
    }

    #[test]
    fn truncated_input_returns_error() {
        let schema = schema_i64_and_bytes();
        let err = decode_row(&[0u8; 2], &schema);
        assert!(matches!(err, Err(PageError::EncodingError(_))));
    }

    #[test]
    fn bytes_overflow_returns_error() {
        let schema = Schema {
            columns: vec!["b".into()],
            types: vec![ColumnType::Bytes],
            primary_key: None,
        };
        // First 4 bytes declare length 1000 but only 2 bytes follow.
        let mut bytes = vec![0u8]; // bitmap
        bytes.extend_from_slice(&1000u32.to_le_bytes());
        bytes.extend_from_slice(&[0xAA, 0xBB]);
        let err = decode_row(&bytes, &schema);
        assert!(matches!(err, Err(PageError::EncodingError(_))));
    }
}
