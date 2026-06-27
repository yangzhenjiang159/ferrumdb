//! B+Tree 索引（内存与持久化）。
//!
//! # 职责
//!
//! - 内存 B+Tree：插入、分裂、范围扫描（阶段 2）
//! - 持久化 B+Tree：节点映射到 `ferrumdb-page` 页（阶段 3）
//! - 二级索引与回表（阶段 6）
//!
//! 见项目文档 `docs/plan.md` 阶段 2–3、阶段 6。

#![deny(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
