//! SQL 解析与简单执行。
//!
//! # 职责
//!
//! - 解析 CREATE / INSERT / SELECT 等最小语句集
//! - 调用 [`ferrumdb_engine::StorageEngine`] 执行
//!
//! 见项目文档 `docs/plan.md` 阶段 8。

#![deny(missing_docs)]

#[cfg(test)]
mod tests {
    use ferrumdb_engine::StorageEngine;

    #[test]
    fn engine_trait_is_accessible_from_sql_crate() {
        fn _assert_object_safe(_: &dyn StorageEngine) {}
    }
}
