//! 事务、Undo Log、Read View 与 MVCC。
//!
//! # 职责
//!
//! - `BEGIN` / `COMMIT` / `ROLLBACK` 与 undo 链
//! - Read View 与非锁定一致性读
//!
//! 见项目文档 `docs/plan.md` 阶段 9–10。

#![deny(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
