//! 存储引擎对外接口（阶段 0：仅 trait 定义，无实现）。

use ferrumdb_page::{Row, Schema, Value};

/// 事务标识符。
///
/// 阶段 9 起用于 `begin` / `commit` / `rollback` 的会话内事务跟踪。
pub type TransactionId = u64;

/// 二级索引元数据。
///
/// - `name`: 索引名，供 [`StorageEngine::get_by_index`] / [`StorageEngine::scan_index`] 定位
/// - `columns`: 索引列的列下标（引用 [`Schema::columns`]，下标非空且须在 schema 内）
/// - `is_unique`: 是否唯一索引
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexMeta {
    /// 索引名（表内唯一）。
    pub name: String,
    /// 构成索引的列下标（引用 schema 的列下标）。
    pub columns: Vec<usize>,
    /// 是否强制唯一性。
    pub is_unique: bool,
}

/// 范围扫描的上下界（闭区间或半开区间，阶段 6 细化语义）。
///
/// - `start`: 起始键（含）；`None` 表示从最小键开始
/// - `end`: 结束键；`None` 表示到最大键结束
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeBound {
    /// 起始键（含）；`None` 表示从最小键开始。
    pub start: Option<Value>,
    /// 结束键（排他）；`None` 表示到最大键结束。
    pub end: Option<Value>,
}

impl RangeBound {
    /// 全表扫描。
    pub fn full() -> Self {
        Self {
            start: None,
            end: None,
        }
    }
}

/// 按主键或索引顺序返回行的迭代器。
pub type RowIterator<'a> = Box<dyn Iterator<Item = Result<Row, EngineError>> + 'a>;

/// 存储引擎层错误。
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// 请求的表不存在。
    #[error("table not found: {0}")]
    TableNotFound(String),

    /// 违反唯一约束（主键或唯一索引冲突）。
    #[error("duplicate key")]
    DuplicateKey,

    /// 按主键定位的行不存在（`update` 等需要「定位已有行」的操作）。
    #[error("row not found: {0}")]
    RowNotFound(String),

    /// 当前阶段尚未实现的能力。
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// 内部错误（I/O、损坏、逻辑断言等）。
    #[error("internal: {0}")]
    Internal(String),
}

/// FerrumDB 存储引擎 trait。
///
/// SQL 层（`ferrumdb-sql`）与集成测试通过此接口访问底层 B+Tree、Buffer Pool 与 WAL。
/// 阶段 0 仅定义接口；具体实现在阶段 7 由 `FerrumEngine` 提供。
///
/// # 实现阶段
///
/// | 方法 | 计划阶段 |
/// |------|----------|
/// | `create_table` / `drop_table` | 7（阶段 6 实现 `create_table`） |
/// | `insert` / `update` / `delete` | 7（阶段 6 实现 `insert`） |
/// | `get_by_pk` | 7（阶段 6 实现） |
/// | `scan` | 6 |
/// | `create_index` / `get_by_index` / `scan_index` | 6 |
/// | `begin` / `commit` / `rollback` | 9 |
pub trait StorageEngine {
    /// 创建一张新表并持久化元数据。
    ///
    /// - `name`: 表名，会话内唯一
    /// - `schema`: 列定义与主键列；引擎据此创建聚簇 B+Tree
    ///
    /// # Errors
    ///
    /// - `EngineError::DuplicateKey` 等价场景：表已存在（可用 `Internal` 或新增变体，阶段 7 定稿）
    /// - `EngineError::Internal` 分配表空间或写 catalog 失败
    fn create_table(&mut self, name: &str, schema: Schema) -> Result<(), EngineError>;

    /// 删除表及其索引、释放相关页（阶段 7）。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    fn drop_table(&mut self, name: &str) -> Result<(), EngineError>;

    /// 插入一行；主键冲突时返回错误。
    ///
    /// 引擎负责更新聚簇索引与所有二级索引（阶段 6 起）。
    fn insert(&mut self, table: &str, row: Row) -> Result<(), EngineError>;

    /// 按主键更新已有行；主键列不可变（`row` 中 pk 列须等于 `pk`）。
    ///
    /// 引擎负责同步维护所有二级索引（索引列值变化时删旧插新）。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    /// - `EngineError::RowNotFound` 表存在但 `pk` 对应的行不存在
    /// - `EngineError::DuplicateKey` 更新后与唯一索引冲突
    /// - `EngineError::Internal` 主键列被改变或编码失败
    fn update(&mut self, table: &str, pk: Value, row: Row) -> Result<(), EngineError>;

    /// 按主键删除一行，并同步删除所有二级索引项。
    ///
    /// 幂等：行不存在返回 `Ok(())`（与 MySQL `DELETE` 0 rows 语义一致）。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    fn delete(&mut self, table: &str, pk: Value) -> Result<(), EngineError>;

    /// 聚簇索引点查：按主键返回完整行。
    fn get_by_pk(&self, table: &str, pk: Value) -> Result<Option<Row>, EngineError>;

    /// 范围扫描：按 `range` 在聚簇索引上顺序返回行。
    ///
    /// 结果按主键升序返回。
    fn scan<'a>(&'a self, table: &str, range: RangeBound) -> Result<RowIterator<'a>, EngineError>;

    /// 在表上创建二级索引。
    ///
    /// - `meta.columns` 中的下标须落在 schema 列数内且非空，否则返回 `Internal`
    /// - 索引名与已存在索引或表内重名时返回 `Internal`
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    /// - `EngineError::Internal` 列下标非法、索引重名或分配页失败
    fn create_index(&mut self, table: &str, meta: IndexMeta) -> Result<(), EngineError>;

    /// 二级索引点查：按索引键返回一条完整行。
    ///
    /// 引擎先查二级索引得到主键，再回表聚簇索引。非唯一索引返回匹配该索引键的
    /// 第一行（主键最小）；键不存在返回 `None`。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    /// - `EngineError::Internal` 索引不存在或索引键无法编码
    fn get_by_index(&self, table: &str, index: &str, key: Value)
        -> Result<Option<Row>, EngineError>;

    /// 二级索引范围扫描：按 `range` 在指定索引上顺序返回行，结果按索引键升序。
    ///
    /// 每条记录内部完成回表（二级 → 主键 → 聚簇取整行）。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    /// - `EngineError::Internal` 索引不存在或边界值无法编码
    fn scan_index<'a>(
        &'a self,
        table: &str,
        index: &str,
        range: RangeBound,
    ) -> Result<RowIterator<'a>, EngineError>;

    /// 开启事务，返回事务 ID（阶段 9）。
    ///
    /// 阶段 7 之前实现可返回 `Unsupported`。
    fn begin(&mut self) -> Result<TransactionId, EngineError>;

    /// 提交事务：redo 落盘、释放锁与 undo（阶段 9）。
    fn commit(&mut self, tx: TransactionId) -> Result<(), EngineError>;

    /// 回滚事务：沿 undo 链恢复（阶段 9）。
    fn rollback(&mut self, tx: TransactionId) -> Result<(), EngineError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 阶段 0 冒烟：trait 可被类型系统完整实现（无需真实存储逻辑）。
    struct StubEngine;

    impl StorageEngine for StubEngine {
        fn create_table(&mut self, _: &str, _: Schema) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("create_table".into()))
        }

        fn drop_table(&mut self, _: &str) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("drop_table".into()))
        }

        fn insert(&mut self, _: &str, _: Row) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("insert".into()))
        }

        fn update(&mut self, _: &str, _: Value, _: Row) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("update".into()))
        }

        fn delete(&mut self, _: &str, _: Value) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("delete".into()))
        }

        fn get_by_pk(&self, _: &str, _: Value) -> Result<Option<Row>, EngineError> {
            Err(EngineError::Unsupported("get_by_pk".into()))
        }

        fn scan<'a>(&'a self, _: &str, _: RangeBound) -> Result<RowIterator<'a>, EngineError> {
            Err(EngineError::Unsupported("scan".into()))
        }

        fn create_index(&mut self, _: &str, _: IndexMeta) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("create_index".into()))
        }

        fn get_by_index(
            &self,
            _: &str,
            _: &str,
            _: Value,
        ) -> Result<Option<Row>, EngineError> {
            Err(EngineError::Unsupported("get_by_index".into()))
        }

        fn scan_index<'a>(
            &'a self,
            _: &str,
            _: &str,
            _: RangeBound,
        ) -> Result<RowIterator<'a>, EngineError> {
            Err(EngineError::Unsupported("scan_index".into()))
        }

        fn begin(&mut self) -> Result<TransactionId, EngineError> {
            Err(EngineError::Unsupported("begin".into()))
        }

        fn commit(&mut self, _: TransactionId) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("commit".into()))
        }

        fn rollback(&mut self, _: TransactionId) -> Result<(), EngineError> {
            Err(EngineError::Unsupported("rollback".into()))
        }
    }

    #[test]
    fn storage_engine_trait_is_object_safe_and_implementable() {
        let mut engine = StubEngine;
        let err = engine.create_table("t", Schema { columns: vec![], types: vec![], primary_key: None }).unwrap_err();
        assert!(matches!(err, EngineError::Unsupported(_)));
    }

    #[test]
    fn range_bound_full_scan() {
        let range = RangeBound::full();
        assert!(range.start.is_none());
        assert!(range.end.is_none());
    }
}
