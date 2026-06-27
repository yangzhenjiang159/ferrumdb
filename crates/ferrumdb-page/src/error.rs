//! 页相关错误。

/// 读写页时的错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PageError {
    /// 输入字节长度不是 16384（[`PAGE_SIZE`]）。
    #[error("invalid page length: expected 16384, got {0}")]
    InvalidLength(usize),

    /// magic 或 footer 标记不匹配，可能不是 FerrumDB 页。
    #[error("invalid page magic")]
    InvalidMagic,

    /// checksum 校验失败，页可能损坏或被篡改。
    #[error("checksum mismatch")]
    ChecksumMismatch,

    /// 无法识别的页类型字节。
    #[error("unknown page type: {0}")]
    UnknownPageType(u8),
}
