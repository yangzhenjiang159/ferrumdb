//! 空闲链表辅助函数。

/// Free page 的 user_data 编码：
///
/// ```text
/// [next_is_some: u8][next_page_id: u32 LE]
/// ```
///
/// 5 字节；其余 user_data 字节保持 0。
use crate::error::SpaceError;

pub(crate) const FREE_PAGE_BYTES: usize = 5;

/// 把 `next` 编码为 5 字节，写入给定缓冲的开头。
pub(crate) fn encode_free(next: Option<u32>) -> [u8; FREE_PAGE_BYTES] {
    let mut out = [0u8; FREE_PAGE_BYTES];
    match next {
        Some(id) => {
            out[0] = 1;
            out[1..5].copy_from_slice(&id.to_le_bytes());
        }
        None => {
            out[0] = 0;
        }
    }
    out
}

/// 从给定缓冲的开头解码 next 指针。
pub(crate) fn decode_free(bytes: &[u8]) -> Result<Option<u32>, SpaceError> {
    if bytes.len() < FREE_PAGE_BYTES {
        return Err(SpaceError::FreeListCorrupted(0));
    }
    if bytes[0] == 0 {
        Ok(None)
    } else {
        let id = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        Ok(Some(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_none() {
        let enc = encode_free(None);
        assert_eq!(decode_free(&enc).unwrap(), None);
    }

    #[test]
    fn encode_decode_some() {
        let enc = encode_free(Some(123));
        assert_eq!(decode_free(&enc).unwrap(), Some(123));
    }

    #[test]
    fn truncated_input_returns_error() {
        let err = decode_free(&[1, 2]);
        assert!(matches!(err, Err(SpaceError::FreeListCorrupted(_))));
    }
}
