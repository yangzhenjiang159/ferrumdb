//! WAL 相关错误。

/// WAL 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum WalError {
    /// 包装的 I/O 错误。
    #[error("wal io: {0}")]
    Io(#[from] std::io::Error),

    /// 单条 record 的 CRC32 校验失败。后续 record 不可信。
    #[error("record crc32 mismatch at lsn {lsn}")]
    RecordCrcMismatch {
        /// 校验失败的 record lsn。
        lsn: u64,
    },

    /// 日志文件末尾 mid-record。**不视为 error**（可能是正在写入；调用方需显式忽略）。
    #[error("wal: log file truncated (incomplete record at end)")]
    Truncated,

    /// record 头部声明了不可能的尺寸（page_id 越界、payload_len 异常）。
    #[error("invalid record: {0}")]
    InvalidRecord(String),

    /// checkpoint 记录的 magic 不匹配。
    #[error("checkpoint record corrupt")]
    CheckpointCorrupt,

    /// LSN 计数器已到 u64::MAX，无法再分配。
    #[error("lsn exhausted (u64::MAX reached)")]
    LsnExhausted,

    /// record 的 lsn 不等于期望的下一个 lsn（文件可能损坏或被截断）。
    #[error("lsn out of order: expected {expected}, got {got}")]
    OutOfOrder {
        /// 期望的下一个 lsn。
        expected: u64,
        /// 实际读到的 lsn。
        got: u64,
    },
}
