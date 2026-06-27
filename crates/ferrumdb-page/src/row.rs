//! 行、列类型与表结构（阶段 1–2 由开发者完善编解码实现）。

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

/// 一行数据，列顺序与 [`Schema::columns`] 一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub values: Vec<Value>,
}

/// 表结构描述（阶段 2 扩展主键、nullable、类型信息）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    pub columns: Vec<String>,
}
