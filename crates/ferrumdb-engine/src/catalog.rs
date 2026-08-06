//! 内存表目录：表名 → 表元数据（schema、聚簇树句柄、二级索引）。
//!
//! 阶段 6 目录为**内存态**（进程重启后需重建），DDL 元数据持久化归阶段 7
//! （见 `docs/plan.md` 阶段 7 与 `ferrumdb-space` superblock 注释）。
//!
//! 设计上直接持有 [`PersistentBtree`] 句柄而非仅存 root page id：`PersistentBtree::open`
//! 会遍历叶子链计算高度/条目数（O(叶子)），每次操作重建句柄代价过高。

use std::collections::HashMap;

use ferrumdb_btree::PersistentBtree;
use ferrumdb_page::Schema;

use crate::engine::{EngineError, IndexMeta};

/// 一张表的一个二级索引（元数据 + 树句柄）。
pub struct IndexEntry {
    /// 索引元数据（列集合 / 唯一性）。
    pub meta: IndexMeta,
    /// 该二级索引对应的 B+Tree 句柄。
    pub tree: PersistentBtree,
}

/// 一张表的元数据。
pub struct TableMeta {
    /// 表名。
    pub name: String,
    /// 表 schema（列定义与主键列）。
    pub schema: Schema,
    /// 聚簇索引（主键）B+Tree 句柄。
    pub clustered: PersistentBtree,
    /// 该表的全部二级索引（按创建顺序）。
    pub indexes: Vec<IndexEntry>,
}

/// 内存表目录。
pub struct TableCatalog {
    tables: HashMap<String, TableMeta>,
}

impl TableCatalog {
    /// 空目录。
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    /// 登记一张新表。
    ///
    /// # Errors
    ///
    /// - `EngineError::Internal` 表名已存在
    pub fn add_table(
        &mut self,
        name: String,
        schema: Schema,
        clustered: PersistentBtree,
    ) -> Result<(), EngineError> {
        if self.tables.contains_key(&name) {
            return Err(EngineError::Internal(format!("table already exists: {name}")));
        }
        self.tables.insert(
            name.clone(),
            TableMeta {
                name,
                schema,
                clustered,
                indexes: Vec::new(),
            },
        );
        Ok(())
    }

    /// 取一张表的不可变引用。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    pub fn get(&self, name: &str) -> Result<&TableMeta, EngineError> {
        self.tables
            .get(name)
            .ok_or_else(|| EngineError::TableNotFound(name.to_string()))
    }

    /// 取一张表的可变引用。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    pub fn get_mut(&mut self, name: &str) -> Result<&mut TableMeta, EngineError> {
        self.tables
            .get_mut(name)
            .ok_or_else(|| EngineError::TableNotFound(name.to_string()))
    }

    /// 为一张表添加二级索引。
    ///
    /// 校验：列下标非空、全部落在 schema 列数内；索引名在表内唯一。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    /// - `EngineError::Internal` 列下标非法或索引名冲突
    pub fn add_index(
        &mut self,
        table: &str,
        meta: IndexMeta,
        tree: PersistentBtree,
    ) -> Result<(), EngineError> {
        let table_meta = self.get_mut(table)?;
        validate_index_meta(&meta, &table_meta.schema)?;
        if table_meta.indexes.iter().any(|e| e.meta.name == meta.name) {
            return Err(EngineError::Internal(format!(
                "index already exists: {} on table {}",
                meta.name, table
            )));
        }
        table_meta.indexes.push(IndexEntry { meta, tree });
        Ok(())
    }

    /// 取出表中某个二级索引的引用。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    /// - `EngineError::Internal` 索引不存在
    pub fn index(&self, table: &str, index: &str) -> Result<&IndexEntry, EngineError> {
        let table_meta = self.get(table)?;
        table_meta
            .indexes
            .iter()
            .find(|e| e.meta.name == index)
            .ok_or_else(|| {
                EngineError::Internal(format!("index not found: {index} on table {table}"))
            })
    }

    /// 取出表中某个二级索引的可变引用。
    ///
    /// # Errors
    ///
    /// - `EngineError::TableNotFound` 表不存在
    /// - `EngineError::Internal` 索引不存在
    pub fn index_mut(
        &mut self,
        table: &str,
        index: &str,
    ) -> Result<&mut IndexEntry, EngineError> {
        let table_meta = self.get_mut(table)?;
        table_meta
            .indexes
            .iter_mut()
            .find(|e| e.meta.name == index)
            .ok_or_else(|| {
                EngineError::Internal(format!("index not found: {index} on table {table}"))
            })
    }
}

impl Default for TableCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// 校验索引元数据：列集合非空且下标合法。
///
/// # Errors
///
/// - `EngineError::Internal` 列下标越界或索引列为空
pub fn validate_index_meta(meta: &IndexMeta, schema: &Schema) -> Result<(), EngineError> {
    if meta.columns.is_empty() {
        return Err(EngineError::Internal(format!(
            "index {} has no columns",
            meta.name
        )));
    }
    for &col in &meta.columns {
        if col >= schema.columns.len() {
            return Err(EngineError::Internal(format!(
                "index {} references column {} out of range (schema has {})",
                meta.name,
                col,
                schema.columns.len()
            )));
        }
    }
    Ok(())
}
