//! Redo Log（WAL）与崩溃恢复。
//!
//! # 职责
//!
//! - append-only redo 记录
//! - checkpoint 与启动 replay
//!
//! 见项目文档 `docs/plan.md` 阶段 5。

#![deny(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
