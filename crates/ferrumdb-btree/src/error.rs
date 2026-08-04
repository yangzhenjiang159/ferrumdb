//! B+Tree 相关错误。

/// B+Tree 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum BTreeError {
    /// `get` 或 `delete` 找不到 key（仅适用于非 UNIQUE 树且调用方选择严格语义时）。
    /// 当前实现中 `get` 返回 `Ok(None)`，此变体预留给未来 UNIQUE-only 模式。
    #[error("key not found")]
    KeyNotFound,

    /// 插入违反唯一约束（v1 未启用，预留）。
    #[error("duplicate key")]
    DuplicateKey,

    /// Wrapped `PageError` from a `ferrumdb-page` operation.
    #[error("page error: {0}")]
    Page(#[from] ferrumdb_page::PageError),

    /// Wrapped `SpaceError` from a `ferrumdb-space` operation.
    #[error("space error: {0}")]
    Space(#[from] ferrumdb_space::SpaceError),

    /// 键比较时发生错误（预留）。
    #[error("comparison failed: {0}")]
    ComparisonFailed(String),

    /// 持久化节点反序列化时遇到未知 kind 字节。
    #[error("invalid node kind: {0}")]
    InvalidNodeKind(u8),

    /// 节点键的数量超出 ORDER 上限（节点页损坏）。
    #[error("node has too many keys: {got} > {max}")]
    TooManyKeys {
        /// 实际读到的 key 数。
        got: usize,
        /// ORDER 上限。
        max: usize,
    },

    /// 节点页 child/value 数量与 keys 不匹配（节点页损坏）。
    #[error("node arity mismatch: keys={keys}, children={children}, values={values}")]
    ArityMismatch {
        /// key 数量。
        keys: usize,
        /// child 数量。
        children: usize,
        /// value 数量。
        values: usize,
    },
}
