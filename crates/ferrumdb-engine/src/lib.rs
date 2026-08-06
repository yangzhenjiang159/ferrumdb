//! 存储引擎统一入口与 [`StorageEngine`] trait。
//!
//! # 职责
//!
//! - 定义对外存储 API（DDL/DML/扫描/事务）
//! - 阶段 6 提供 `FerrumEngine` 最小实现（内存 catalog + Space 持久树）
//!
//! 见项目文档 `docs/plan.md` 阶段 0、阶段 6–7。

#![deny(missing_docs)]

mod catalog;
mod engine;
mod ferrum_engine;

#[cfg(test)]
mod integration;

pub use catalog::{IndexEntry, TableCatalog, TableMeta};
pub use engine::{
    EngineError, IndexMeta, RangeBound, RowIterator, StorageEngine, TransactionId,
};
pub use ferrum_engine::FerrumEngine;
