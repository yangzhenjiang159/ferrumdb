//! `Wal` 主结构。

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::WalError;
use crate::record::{RedoRecord, CHECKPOINT_MAGIC};

/// 8 字节文件头（`next_lsn`）。
pub const HEADER_BYTES: usize = 8;

/// checkpoint 固定 slot：12 字节。
pub const CHECKPOINT_SLOT_BYTES: usize = 12;

/// 文件布局：
/// ```text
/// [header: 8B]      next_lsn (u64 LE)
/// [checkpoint: 12B] magic (u32 LE = 0xFEEDC0DE) + max_flushed_lsn (u64 LE)
/// [record 0]
/// [record 1]
/// ...
/// ```
/// 固定 slot 让 checkpoint 不需要 scan 整个文件。
const DATA_OFFSET: usize = HEADER_BYTES + CHECKPOINT_SLOT_BYTES;

/// WAL 抽象：append-only redo log + checkpoint + recover。
pub struct Wal {
    file: File,
    path: PathBuf,
    next_lsn: u64,
    checkpoint_lsn: u64,
}

impl Wal {
    /// 创建新 WAL 文件。Header = next_lsn=1, checkpoint slot = 全 0。
    pub fn create(path: impl AsRef<Path>) -> Result<Self, WalError> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        // Write header (8B) + zero checkpoint slot (12B).
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&1u64.to_le_bytes())?;
        file.write_all(&[0u8; CHECKPOINT_SLOT_BYTES])?;
        file.sync_all()?;
        Ok(Self {
            file,
            path,
            next_lsn: 1,
            checkpoint_lsn: 0,
        })
    }

    /// 打开已有 WAL 文件，从固定位置恢复 next_lsn 和 checkpoint_lsn。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, WalError> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
        let bytes = read_all(&mut file)?;
        if bytes.len() < DATA_OFFSET {
            return Err(WalError::InvalidRecord(
                "WAL file shorter than header + checkpoint slot".into(),
            ));
        }
        // Header
        let header_next_lsn = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        // Checkpoint slot
        let mut checkpoint_lsn = 0u64;
        let cp_magic = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        if cp_magic == CHECKPOINT_MAGIC {
            checkpoint_lsn = u64::from_le_bytes([
                bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
            ]);
        }
        // Scan records from DATA_OFFSET to EOF
        let mut last_lsn: Option<u64> = None;
        let mut pos = DATA_OFFSET;
        while pos < bytes.len() {
            let expected = last_lsn.map(|l| l + 1).unwrap_or(1);
            match RedoRecord::decode(&bytes[pos..], Some(expected)) {
                Ok(rec) => {
                    pos += rec.encoded_len();
                    last_lsn = Some(rec.lsn);
                }
                Err(WalError::Truncated) => break,
                Err(_) => break,
            }
        }
        let scanned_next_lsn = last_lsn.map(|l| l + 1).unwrap_or(1);
        let next_lsn = header_next_lsn.max(scanned_next_lsn);
        if scanned_next_lsn > header_next_lsn + 1 {
            return Err(WalError::InvalidRecord(format!(
                "header next_lsn {} too far behind scanned {}",
                header_next_lsn, scanned_next_lsn
            )));
        }
        file.seek(SeekFrom::Start(0))?;
        Ok(Self {
            file,
            path,
            next_lsn,
            checkpoint_lsn,
        })
    }

    /// 下一个 lsn。
    pub fn next_lsn(&self) -> u64 {
        self.next_lsn
    }

    /// checkpoint 时的 max_flushed_lsn。
    pub fn checkpoint_lsn(&self) -> u64 {
        self.checkpoint_lsn
    }

    /// WAL 文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 追加一条 redo record。
    ///
    /// # Errors
    ///
    /// - `WalError::LsnExhausted` 当 lsn 达到 `u64::MAX`
    /// - `WalError::Io` 写文件失败
    pub fn append(
        &mut self,
        page_id: u32,
        offset: u32,
        payload: &[u8],
    ) -> Result<u64, WalError> {
        let lsn = self.next_lsn;
        if lsn == u64::MAX {
            return Err(WalError::LsnExhausted);
        }
        let record = RedoRecord {
            lsn,
            page_id,
            offset,
            payload: payload.to_vec(),
        };
        let bytes = record.encode();
        // Append record at end.
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.next_lsn = lsn.checked_add(1).ok_or(WalError::LsnExhausted)?;
        // Rewrite header (8B) to reflect new next_lsn.
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&self.next_lsn.to_le_bytes())?;
        self.file.sync_all()?;
        Ok(lsn)
    }

    /// 显式 fsync（当前实现中 `append` 已经 fsync；保留此方法给未来的 batch 优化）。
    pub fn fsync(&mut self) -> Result<(), WalError> {
        self.file.sync_all()?;
        Ok(())
    }

    /// 写入 checkpoint 记录到固定 slot。
    ///
    /// # Errors
    ///
    /// - `WalError::Io` 写文件失败
    pub fn checkpoint(&mut self, max_flushed_lsn: u64) -> Result<(), WalError> {
        self.file.seek(SeekFrom::Start(HEADER_BYTES as u64))?;
        self.file.write_all(&CHECKPOINT_MAGIC.to_le_bytes())?;
        self.file
            .write_all(&max_flushed_lsn.to_le_bytes())?;
        self.file.sync_all()?;
        self.checkpoint_lsn = max_flushed_lsn;
        Ok(())
    }

    /// Replay records after `checkpoint_lsn` to `target`.
    ///
    /// # Errors
    ///
    /// - `WalError::RecordCrcMismatch` 单条 record 损坏
    /// - `WalError::Io` 读文件失败
    pub fn recover<F>(&mut self, mut target: F) -> Result<u64, WalError>
    where
        F: FnMut(&RedoRecord) -> Result<(), WalError>,
    {
        let bytes = read_all(&mut self.file)?;
        if bytes.len() < DATA_OFFSET {
            return Ok(0);
        }
        let mut last_lsn = 0u64;
        let mut pos = DATA_OFFSET;
        while pos < bytes.len() {
            let expected = if last_lsn == 0 { 1 } else { last_lsn + 1 };
            match RedoRecord::decode(&bytes[pos..], Some(expected)) {
                Ok(rec) => {
                    pos += rec.encoded_len();
                    last_lsn = rec.lsn;
                    if rec.lsn > self.checkpoint_lsn {
                        target(&rec)?;
                    }
                }
                Err(WalError::Truncated) => break,
                Err(WalError::RecordCrcMismatch { lsn }) => {
                    return Err(WalError::RecordCrcMismatch { lsn });
                }
                Err(e) => return Err(e),
            }
        }
        Ok(last_lsn)
    }
}

fn read_all(file: &mut File) -> Result<Vec<u8>, WalError> {
    let len = file.metadata()?.len() as usize;
    file.seek(SeekFrom::Start(0))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;
    Ok(buf)
}
