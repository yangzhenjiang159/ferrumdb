//! 存储引擎统一入口与 [`StorageEngine`] trait。
//!
//! # 职责
//!
//! - 定义对外存储 API（DDL/DML/扫描/事务）
//! - 阶段 7 起提供 `FerrumEngine` 默认实现，串联 btree / buffer / wal / space
//!
//! 见项目文档 `docs/plan.md` 阶段 0、阶段 7。

mod engine;

pub use engine::{
    EngineError, RangeBound, RowIterator, StorageEngine, TransactionId,
};
