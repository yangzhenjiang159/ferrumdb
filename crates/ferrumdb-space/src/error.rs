//! 表空间相关错误。

/// 表空间操作错误。
#[derive(Debug, thiserror::Error)]
pub enum SpaceError {
    /// 包装的 I/O 错误。
    #[error("space io: {0}")]
    Io(#[from] std::io::Error),

    /// 请求的 `PageId` 超出当前文件长度。
    #[error("page id out of range: {0}")]
    PageIdOutOfRange(u32),

    /// 空闲链表 next 指针越界或成环。
    #[error("free list corrupted at page {0}")]
    FreeListCorrupted(u32),

    /// Superblock magic 与 [`PAGE_MAGIC`](ferrumdb_page::PAGE_MAGIC) 不一致。
    #[error("superblock invalid magic")]
    SuperblockInvalidMagic,

    /// 文件中的 page_size 与编译期 `PAGE_SIZE` 不一致。
    #[error("superblock page size mismatch: file {file}, build {build}")]
    SuperblockPageSizeMismatch {
        /// 文件中声明的 page_size。
        file: u32,
        /// 编译期 `PAGE_SIZE`。
        build: u32,
    },

    /// 文件版本高于本二进制所支持的最高版本。
    #[error("superblock version {0} not supported")]
    SuperblockVersionUnsupported(u32),

    /// Space 未调用 `open` / `create` 就使用了。
    #[error("space not initialized")]
    NotInitialized,

    /// 用户数据长度不足以容纳 Superblock。
    #[error("superblock truncated: {got} bytes")]
    SuperblockTruncated {
        /// 实际得到的字节数。
        got: usize,
    },
}
