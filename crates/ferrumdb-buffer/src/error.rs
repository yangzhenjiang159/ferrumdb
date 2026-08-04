//! Buffer Pool 相关错误。

/// Buffer Pool 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    /// 包装的 I/O 错误。
    #[error("buffer io: {0}")]
    Io(#[from] std::io::Error),

    /// Wrapped `PageError` from `ferrumdb-page`.
    #[error("page error: {0}")]
    Page(#[from] ferrumdb_page::PageError),

    /// Wrapped `SpaceError` from `ferrumdb-space`.
    #[error("space error: {0}")]
    Space(#[from] ferrumdb_space::SpaceError),

    /// 所有 frame 都被 pinned 或 dirty，无法淘汰。
    #[error("buffer pool full (all frames pinned or dirty)")]
    PoolFull,

    /// 内部状态错误：frame_id 超出范围或 frame 未关联 page。
    #[error("frame not found: {0}")]
    FrameNotFound(usize),

    /// 用户试图通过 `BufferPoolSource` 写入一个未 pinned 的页。
    #[error("page {0} not in pool")]
    PageNotInPool(u32),
}
