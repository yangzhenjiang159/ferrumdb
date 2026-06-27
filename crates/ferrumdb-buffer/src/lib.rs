//! Buffer Pool：页缓存、pin、LRU、脏页刷盘。
//!
//! # 职责
//!
//! - 将 `ferrumdb-space` 的磁盘页缓存在内存
//! - 提供 pin/unpin、脏页 flush、LRU 淘汰
//!
//! 见项目文档 `docs/plan.md` 阶段 4。

#![deny(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
