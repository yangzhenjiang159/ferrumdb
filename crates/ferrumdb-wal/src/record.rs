//! Redo record 编解码。

use crc32fast::Hasher;

use crate::error::WalError;

/// 4 字节 magic 标识 checkpoint 记录。
pub const CHECKPOINT_MAGIC: u32 = 0xFEEDC0DE;

/// WAL record 头部固定长度（lsn + page_id + offset + payload_len）。
const RECORD_HEADER_BYTES: usize = 8 + 4 + 4 + 4;

/// CRC32 校验覆盖范围之后的 4 字节。
const CRC_BYTES: usize = 4;

/// 单条 redo 记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedoRecord {
    /// 日志序列号；全局单调递增。
    pub lsn: u64,
    /// 目标页 id。
    pub page_id: u32,
    /// payload 在目标页中的写入偏移（字节）。
    pub offset: u32,
    /// 写入的字节序列。
    pub payload: Vec<u8>,
}

impl RedoRecord {
    /// 序列化为字节（含 CRC32 尾部）。
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RECORD_HEADER_BYTES + self.payload.len() + CRC_BYTES);
        out.extend_from_slice(&self.lsn.to_le_bytes());
        out.extend_from_slice(&self.page_id.to_le_bytes());
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload);
        let mut h = Hasher::new();
        h.update(&out);
        out.extend_from_slice(&h.finalize().to_le_bytes());
        out
    }

    /// 从字节流解码（已剥离 8 字节文件 header）。
    ///
    /// # Errors
    ///
    /// - `Truncated`（不视为 fatal）如果字节不够一个 record 头部
    /// - `RecordCrcMismatch` 如果 CRC 校验失败
    /// - `OutOfOrder` 如果调用方传入 expected_lsn 且与 record.lsn 不一致
    pub fn decode(bytes: &[u8], expected_lsn: Option<u64>) -> Result<Self, WalError> {
        if bytes.len() < RECORD_HEADER_BYTES + CRC_BYTES {
            return Err(WalError::Truncated);
        }
        let lsn = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        if let Some(expected) = expected_lsn {
            if lsn != expected {
                return Err(WalError::OutOfOrder { expected, got: lsn });
            }
        }
        let page_id = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let offset = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let payload_len =
            u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;

        let body_end = RECORD_HEADER_BYTES + payload_len;
        let crc_end = body_end + CRC_BYTES;
        if bytes.len() < crc_end {
            return Err(WalError::Truncated);
        }

        let payload = bytes[RECORD_HEADER_BYTES..body_end].to_vec();
        let stored_crc = u32::from_le_bytes([
            bytes[body_end],
            bytes[body_end + 1],
            bytes[body_end + 2],
            bytes[body_end + 3],
        ]);
        let mut h = Hasher::new();
        h.update(&bytes[..body_end]);
        let computed = h.finalize();
        if stored_crc != computed {
            return Err(WalError::RecordCrcMismatch { lsn });
        }

        Ok(RedoRecord {
            lsn,
            page_id,
            offset,
            payload,
        })
    }

    /// 序列化后总字节数。
    pub fn encoded_len(&self) -> usize {
        RECORD_HEADER_BYTES + self.payload.len() + CRC_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let r = RedoRecord {
            lsn: 42,
            page_id: 7,
            offset: 100,
            payload: b"hello world".to_vec(),
        };
        let bytes = r.encode();
        let decoded = RedoRecord::decode(&bytes, Some(42)).unwrap();
        assert_eq!(decoded, r);
    }

    #[test]
    fn decode_with_wrong_expected_lsn() {
        let r = RedoRecord {
            lsn: 42,
            page_id: 0,
            offset: 0,
            payload: vec![],
        };
        let bytes = r.encode();
        let err = RedoRecord::decode(&bytes, Some(99)).unwrap_err();
        assert!(matches!(err, WalError::OutOfOrder { .. }));
    }

    #[test]
    fn decode_truncated_returns_truncated_error() {
        let bytes = vec![0u8; 10]; // less than header
        let err = RedoRecord::decode(&bytes, None).unwrap_err();
        assert!(matches!(err, WalError::Truncated));
    }

    #[test]
    fn decode_crc_mismatch() {
        let r = RedoRecord {
            lsn: 1,
            page_id: 0,
            offset: 0,
            payload: vec![0xAB, 0xCD],
        };
        let mut bytes = r.encode();
        // Corrupt a payload byte.
        bytes[20] ^= 0xFF;
        let err = RedoRecord::decode(&bytes, Some(1)).unwrap_err();
        assert!(matches!(err, WalError::RecordCrcMismatch { lsn: 1 }));
    }

    #[test]
    fn empty_payload_round_trip() {
        let r = RedoRecord {
            lsn: 1,
            page_id: 5,
            offset: 0,
            payload: vec![],
        };
        let bytes = r.encode();
        let decoded = RedoRecord::decode(&bytes, Some(1)).unwrap();
        assert_eq!(decoded, r);
        assert_eq!(r.encoded_len(), RECORD_HEADER_BYTES + CRC_BYTES);
    }
}
