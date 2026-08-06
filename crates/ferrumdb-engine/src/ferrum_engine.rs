//! `StorageEngine` trait 的最小实现 `FerrumEngine`（阶段 6）。
//!
//! 只实现本阶段所需方法子集：`create_table` / `create_index` / `insert` /
//! `get_by_pk` / `get_by_index` / `scan` / `scan_index`；其余（`update` / `delete` /
//! `drop_table` / 事务）返回 `EngineError::Unsupported`，随阶段 7/9 落地。
//!
//! 架构上复用 `ferrumdb-space::Space`（PageSource）直连 + 现有
//! `ferrumdb-btree::PersistentBtree`；`BufferPool` / WAL 接入归阶段 7。
//!
//! # 线程模型
//!
//! 单线程 `&mut` 语义（见 `docs/plan.md` 阶段 4 死锁约定）；`Space` 经
//! [`RefCell`] 提供内部可变性，使 `&self` 的读方法也能通过 `&mut Space`
//! 驱动 `PersistentBtree` 的读写。

use std::cell::RefCell;

use ferrumdb_btree::PersistentBtree;
use ferrumdb_page::{
    decode_key, decode_row, encode_index_key, encode_key, encode_pk, encode_row,
    encode_secondary_key, primary_key_type, successor, upper_bound, ColumnType, Row, Schema,
    Value,
};
use ferrumdb_space::Space;

use crate::catalog::{TableCatalog, validate_index_meta};
use crate::engine::{
    EngineError, IndexMeta, RangeBound, RowIterator, StorageEngine, TransactionId,
};

/// 每张表：1 个聚簇 B+Tree + N 个二级 B+Tree（见 `catalog.rs`）。
pub struct FerrumEngine {
    /// 表空间文件（单线程使用，经 RefCell 提供 `&self` 方法所需的 `&mut Space`）。
    space: RefCell<Space>,
    /// 内存表目录（DDL 元数据持久化归阶段 7）。
    catalog: TableCatalog,
}

impl FerrumEngine {
    /// 以已打开的表空间构造引擎。
    pub fn new(space: Space) -> Self {
        Self {
            space: RefCell::new(space),
            catalog: TableCatalog::new(),
        }
    }

    /// 从文件路径打开/创建表空间并构造引擎。
    pub fn open_or_create(path: impl AsRef<std::path::Path>) -> Result<Self, EngineError> {
        let path = path.as_ref();
        let space = if path.exists() {
            match Space::open(path) {
                Ok(s) => s,
                Err(ferrumdb_space::SpaceError::SuperblockInvalidMagic) => {
                    Space::create(path).map_err(space_err)?
                }
                Err(e) => return Err(space_err(e)),
            }
        } else {
            Space::create(path).map_err(space_err)?
        };
        Ok(Self::new(space))
    }

    /// 表的主键列下标（用于 insert 取主键值）。
    fn pk_index(&self, table: &str) -> Result<usize, EngineError> {
        self.catalog
            .get(table)?
            .schema
            .primary_key
            .ok_or_else(|| EngineError::Internal("table has no primary key".into()))
    }

    /// 从一行中取出索引列的值。
    fn index_values(meta: &IndexMeta, row: &Row) -> Result<Vec<Value>, EngineError> {
        meta.columns
            .iter()
            .map(|&c| {
                row.values
                    .get(c)
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::Internal(format!(
                            "row has {} values, index column {} out of range",
                            row.values.len(),
                            c
                        ))
                    })
            })
            .collect()
    }

    /// 二级索引列的类型（decode 前缀扫描边界时用）。
    fn index_column_types(&self, table: &str, index: &str) -> Result<Vec<ColumnType>, EngineError> {
        let meta = self.catalog.index(table, index)?;
        let schema = &self.catalog.get(table)?.schema;
        Ok(meta.meta.columns.iter().map(|&c| schema.types[c]).collect())
    }

    /// 表内全部 B+Tree 的 root page id（聚簇 + 各二级），供持久化验收重新打开树。
    pub fn root_ids(&self, table: &str) -> Result<(u32, Vec<(String, u32)>), EngineError> {
        let meta = self.catalog.get(table)?;
        let clustered = meta.clustered.root_page_id();
        let indexes = meta
            .indexes
            .iter()
            .map(|e| (e.meta.name.clone(), e.tree.root_page_id()))
            .collect();
        Ok((clustered, indexes))
    }

    /// 表空间当前包含的页数（调试/测试用）。
    pub fn page_count(&self) -> u32 {
        self.space.borrow().page_count()
    }

    /// 探测唯一索引：二级树中是否已存在以 `index_key` 开头的 key。
    ///
    /// 由于编码前缀无关，`scan_range(P, successor(P))` 精确返回所有以 `P` 开头的
    /// 二级 key；非空即冲突。
    fn unique_index_conflict(
        space: &mut Space,
        entry: &ferrumdb_btree::PersistentBtree,
        index_key: &[u8],
    ) -> Result<bool, EngineError> {
        let end = successor(index_key).ok_or_else(|| {
            EngineError::Internal("index key has no finite successor".into())
        })?;
        let hits = entry.scan_range(space, index_key, &end).map_err(btree_err)?;
        Ok(!hits.is_empty())
    }
}

impl StorageEngine for FerrumEngine {
    fn create_table(&mut self, name: &str, schema: Schema) -> Result<(), EngineError> {
        if schema.primary_key.is_none() {
            return Err(EngineError::Internal(
                "create_table requires a primary key (phase 6)".into(),
            ));
        }
        let tree = {
            let mut space = self.space.borrow_mut();
            PersistentBtree::create(&mut *space).map_err(btree_err)?
        };
        self.catalog.add_table(name.into(), schema, tree)
    }

    fn drop_table(&mut self, name: &str) -> Result<(), EngineError> {
        // 1. 从 catalog 取走表元数据（含全部树句柄）。
        let meta = self.catalog.remove(name)?;
        // 2. 收集聚簇 + 全部二级树的节点页 id（各树内部已去重；树间页 id 不重叠）。
        let mut page_ids = Vec::new();
        {
            let mut space = self.space.borrow_mut();
            page_ids.extend(
                meta.clustered
                    .all_node_page_ids(&mut *space)
                    .map_err(btree_err)?,
            );
            for entry in &meta.indexes {
                page_ids.extend(
                    entry
                        .tree
                        .all_node_page_ids(&mut *space)
                        .map_err(btree_err)?,
                );
            }
        }
        // 3. 释放所有页（跳过页 0 superblock，free_page 内部也会拒绝）。
        let mut space = self.space.borrow_mut();
        for page_id in page_ids {
            space.free_page(page_id).map_err(space_err)?;
        }
        Ok(())
    }

    fn insert(&mut self, table: &str, row: Row) -> Result<(), EngineError> {
        let schema = self.catalog.get(table)?.schema.clone();
        if row.values.len() != schema.columns.len() {
            return Err(EngineError::Internal(format!(
                "row has {} values, schema expects {}",
                row.values.len(),
                schema.columns.len()
            )));
        }
        let pk_idx = self.pk_index(table)?;
        let pk_value = row.values[pk_idx].clone();
        let pk_bytes = encode_pk(&row, &schema).map_err(page_err)?;

        // 1. 聚簇冲突探测（R9：先探测后写入，保证无部分写入）。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get(table)?;
            if meta.clustered.get(&mut *space, &pk_bytes).map_err(btree_err)?.is_some() {
                return Err(EngineError::DuplicateKey);
            }
        }

        // 2. 所有唯一二级索引冲突探测。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get(table)?;
            for entry in &meta.indexes {
                if !entry.meta.is_unique {
                    continue;
                }
                let index_values = Self::index_values(&entry.meta, &row)?;
                let index_key = encode_index_key(&index_values);
                if Self::unique_index_conflict(&mut space, &entry.tree, &index_key)? {
                    return Err(EngineError::DuplicateKey);
                }
            }
        }

        // 3. 写聚簇（root 分裂则句柄内的 root_page_id 自动更新）。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get_mut(table)?;
            let row_bytes = encode_row(&row, &schema).map_err(page_err)?;
            meta.clustered
                .insert(&mut *space, pk_bytes.clone(), row_bytes)
                .map_err(btree_err)?;
        }

        // 4. 写所有二级索引（复合 key = index_key ∥ pk，value = pk_bytes）。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get_mut(table)?;
            for entry in &mut meta.indexes {
                let index_values = Self::index_values(&entry.meta, &row)?;
                let full_key = encode_secondary_key(&index_values, &pk_value);
                entry
                    .tree
                    .insert(&mut *space, full_key, pk_bytes.clone())
                    .map_err(btree_err)?;
            }
        }
        Ok(())
    }

    fn update(&mut self, table: &str, pk: Value, row: Row) -> Result<(), EngineError> {
        let schema = self.catalog.get(table)?.schema.clone();
        if row.values.len() != schema.columns.len() {
            return Err(EngineError::Internal(format!(
                "row has {} values, schema expects {}",
                row.values.len(),
                schema.columns.len()
            )));
        }
        let pk_idx = self.pk_index(table)?;
        // 主键列不可变（KD2）：新行 pk 列必须等于参数 pk。
        if row.values[pk_idx] != pk {
            return Err(EngineError::Internal(
                "update cannot change the primary key column".into(),
            ));
        }
        let pk_bytes = encode_pk(&row, &schema).map_err(page_err)?;

        // 1. 定位旧行；不存在 → RowNotFound。
        let old_row = {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get(table)?;
            match meta.clustered.get(&mut *space, &pk_bytes).map_err(btree_err)? {
                Some(bytes) => decode_row(&bytes, &schema).map_err(page_err)?,
                None => return Err(EngineError::RowNotFound(format!("pk = {pk:?}"))),
            }
        };

        // 2. 唯一索引探测（针对索引列值会变化且唯一索引可能撞键的场景）。
        //    对每个唯一索引，若新旧索引值不同，探测新值是否已存在（先探测后写）。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get(table)?;
            for entry in &meta.indexes {
                if !entry.meta.is_unique {
                    continue;
                }
                let old_vals = Self::index_values(&entry.meta, &old_row)?;
                let new_vals = Self::index_values(&entry.meta, &row)?;
                if old_vals == new_vals {
                    continue;
                }
                let new_key = encode_index_key(&new_vals);
                if Self::unique_index_conflict(&mut space, &entry.tree, &new_key)? {
                    return Err(EngineError::DuplicateKey);
                }
            }
        }

        // 3. 二级索引：索引列值变化 → 删旧插新。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get_mut(table)?;
            for entry in &mut meta.indexes {
                let old_vals = Self::index_values(&entry.meta, &old_row)?;
                let new_vals = Self::index_values(&entry.meta, &row)?;
                if old_vals == new_vals {
                    continue;
                }
                let old_full_key = encode_secondary_key(&old_vals, &pk);
                if !entry.tree.delete(&mut *space, &old_full_key).map_err(btree_err)? {
                    return Err(EngineError::Internal(format!(
                        "update: secondary index {} missing old entry",
                        entry.meta.name
                    )));
                }
                let new_full_key = encode_secondary_key(&new_vals, &pk);
                entry
                    .tree
                    .insert(&mut *space, new_full_key, pk_bytes.clone())
                    .map_err(btree_err)?;
            }
        }

        // 4. 聚簇覆盖写新行。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get_mut(table)?;
            let new_bytes = encode_row(&row, &schema).map_err(page_err)?;
            meta.clustered
                .insert(&mut *space, pk_bytes, new_bytes)
                .map_err(btree_err)?;
        }
        Ok(())
    }

    fn delete(&mut self, table: &str, pk: Value) -> Result<(), EngineError> {
        let schema = self.catalog.get(table)?.schema.clone();
        let pk_bytes = encode_key(&pk);

        // 1. 定位旧行；不存在 → 幂等 Ok(())。
        let old_row = {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get(table)?;
            match meta.clustered.get(&mut *space, &pk_bytes).map_err(btree_err)? {
                Some(bytes) => decode_row(&bytes, &schema).map_err(page_err)?,
                None => return Ok(()),
            }
        };

        // 2. 删所有二级索引项。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get_mut(table)?;
            for entry in &mut meta.indexes {
                let index_vals = Self::index_values(&entry.meta, &old_row)?;
                let full_key = encode_secondary_key(&index_vals, &pk);
                if !entry.tree.delete(&mut *space, &full_key).map_err(btree_err)? {
                    return Err(EngineError::Internal(format!(
                        "delete: secondary index {} missing entry",
                        entry.meta.name
                    )));
                }
            }
        }

        // 3. 删聚簇行。
        {
            let mut space = self.space.borrow_mut();
            let meta = self.catalog.get_mut(table)?;
            if !meta.clustered.delete(&mut *space, &pk_bytes).map_err(btree_err)? {
                return Err(EngineError::Internal(
                    "delete: clustered tree missing row".into(),
                ));
            }
        }
        Ok(())
    }

    fn get_by_pk(&self, table: &str, pk: Value) -> Result<Option<Row>, EngineError> {
        let mut space = self.space.borrow_mut();
        let meta = self.catalog.get(table)?;
        let pk_enc = encode_key(&pk);
        match meta.clustered.get(&mut *space, &pk_enc).map_err(btree_err)? {
            Some(bytes) => Ok(Some(decode_row(&bytes, &meta.schema).map_err(page_err)?)),
            None => Ok(None),
        }
    }

    fn scan<'a>(&'a self, table: &str, range: RangeBound) -> Result<RowIterator<'a>, EngineError> {
        let mut space = self.space.borrow_mut();
        let meta = self.catalog.get(table)?;
        let pk_type = primary_key_type(&meta.schema).map_err(page_err)?;
        let start_enc = range
            .start
            .as_ref()
            .map(encode_key)
            .unwrap_or_default();
        let end_enc = match &range.end {
            Some(v) => encode_key(v),
            None => upper_bound(pk_type),
        };
        let hits = meta
            .clustered
            .scan_range(&mut *space, &start_enc, &end_enc)
            .map_err(btree_err)?;
        let mut rows = Vec::with_capacity(hits.len());
        for (_k, row_bytes) in hits {
            rows.push(decode_row(&row_bytes, &meta.schema).map_err(page_err)?);
        }
        Ok(Box::new(rows.into_iter().map(Ok)))
    }

    fn create_index(&mut self, table: &str, meta: IndexMeta) -> Result<(), EngineError> {
        validate_index_meta(&meta, &self.catalog.get(table)?.schema)?;
        let tree = {
            let mut space = self.space.borrow_mut();
            PersistentBtree::create(&mut *space).map_err(btree_err)?
        };
        self.catalog.add_index(table, meta, tree)
    }

    fn get_by_index(
        &self,
        table: &str,
        index: &str,
        key: Value,
    ) -> Result<Option<Row>, EngineError> {
        let mut space = self.space.borrow_mut();
        let meta = self.catalog.get(table)?;
        let entry = self.catalog.index(table, index)?;
        let pk_type = primary_key_type(&meta.schema).map_err(page_err)?;
        let p = encode_index_key(std::slice::from_ref(&key));
        let end = successor(&p).ok_or_else(|| {
            EngineError::Internal("index key has no finite successor".into())
        })?;
        let hits = entry
            .tree
            .scan_range(&mut *space, &p, &end)
            .map_err(btree_err)?;
        match hits.into_iter().next() {
            // hits 按 (index_key, pk) 升序，首个即最小 pk。
            Some((_, pk_bytes)) => {
                let (pk, _) = decode_key(&pk_bytes, pk_type).map_err(page_err)?;
                let pk_enc = encode_key(&pk);
                match meta.clustered.get(&mut *space, &pk_enc).map_err(btree_err)? {
                    Some(row_bytes) => {
                        Ok(Some(decode_row(&row_bytes, &meta.schema).map_err(page_err)?))
                    }
                    None => Err(EngineError::Internal(
                        "secondary index points to missing primary key row".into(),
                    )),
                }
            }
            None => Ok(None),
        }
    }

    fn scan_index<'a>(
        &'a self,
        table: &str,
        index: &str,
        range: RangeBound,
    ) -> Result<RowIterator<'a>, EngineError> {
        let mut space = self.space.borrow_mut();
        let meta = self.catalog.get(table)?;
        let entry = self.catalog.index(table, index)?;
        let pk_type = primary_key_type(&meta.schema).map_err(page_err)?;
        // 边界编码到第一个索引列（复合索引按首列前缀范围）。
        let first_col_type = self.index_column_types(table, index)?[0];
        let start_enc = range
            .start
            .as_ref()
            .map(|v| encode_index_key(std::slice::from_ref(v)))
            .unwrap_or_default();
        let end_enc = match &range.end {
            Some(v) => encode_index_key(std::slice::from_ref(v)),
            None => upper_bound(first_col_type),
        };
        let hits = entry
            .tree
            .scan_range(&mut *space, &start_enc, &end_enc)
            .map_err(btree_err)?;
        let mut rows = Vec::with_capacity(hits.len());
        for (_k, pk_bytes) in hits {
            let (pk, _) = decode_key(&pk_bytes, pk_type).map_err(page_err)?;
            let pk_enc = encode_key(&pk);
            let row_bytes = meta
                .clustered
                .get(&mut *space, &pk_enc)
                .map_err(btree_err)?
                .ok_or_else(|| {
                    EngineError::Internal(
                        "secondary index points to missing primary key row".into(),
                    )
                })?;
            rows.push(decode_row(&row_bytes, &meta.schema).map_err(page_err)?);
        }
        Ok(Box::new(rows.into_iter().map(Ok)))
    }

    fn begin(&mut self) -> Result<TransactionId, EngineError> {
        Err(EngineError::Unsupported("begin (phase 9)".into()))
    }

    fn commit(&mut self, _tx: TransactionId) -> Result<(), EngineError> {
        Err(EngineError::Unsupported("commit (phase 9)".into()))
    }

    fn rollback(&mut self, _tx: TransactionId) -> Result<(), EngineError> {
        Err(EngineError::Unsupported("rollback (phase 9)".into()))
    }
}

fn btree_err(e: ferrumdb_btree::BTreeError) -> EngineError {
    EngineError::Internal(format!("btree: {e}"))
}

fn space_err(e: ferrumdb_space::SpaceError) -> EngineError {
    EngineError::Internal(format!("space: {e}"))
}

fn page_err(e: ferrumdb_page::PageError) -> EngineError {
    EngineError::Internal(format!("page: {e}"))
}
