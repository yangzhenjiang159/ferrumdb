//! 表空间文件管理与页分配。
//!
//! # 职责
//!
//! - 表空间文件布局（superblock、页号 → 文件偏移）
//! - 空闲页分配与回收
//!
//! 见项目文档 `docs/plan.md` 阶段 3。

#![deny(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
