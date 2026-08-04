//! Superblock：表空间的元数据，存放在 page 0 的 user_data 中。

use ferrumdb_page::{Page, PageType, PAGE_MAGIC, PAGE_SIZE};

use crate::error::SpaceError;

/// Superblock 固定长度（little-endian 编码）。
const SUPERBLOCK_BYTES: usize = 26;

const OFF_MAGIC: usize = 0;            // u32
const OFF_VERSION: usize = 4;          // u32
const OFF_PAGE_SIZE: usize = 8;        // u32
const OFF_FREE_HEAD_SOME: usize = 12;  // u8
const OFF_FREE_HEAD: usize = 13;       // u32
const OFF_ROOT_SOME: usize = 17;       // u8
const OFF_ROOT: usize = 18;            // u32
const OFF_LAST_LSN: usize = 22;        // u64

/// 表空间元数据。
///
/// 存放在 page 0 的 user_data 中；page 自身的 32-byte header + 8-byte footer
/// 仍然提供 CRC32 校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Superblock {
    /// `PAGE_MAGIC` 副本，用于独立校验（不依赖 page header）。
    pub magic: u32,
    /// Superblock 版本；任何不向后兼容的布局变更必须 bump 此字段。
    pub version: u32,
    /// 编译期 `PAGE_SIZE` 副本，校验与运行时一致。
    pub page_size: u32,
    /// 空闲链表头；`None` 表示链表为空。
    pub free_list_head: Option<u32>,
    /// 阶段 3 临时借用此字段记录 B+Tree root；阶段 5 起会有独立的 catalog 页。
    pub root_page_id: Option<u32>,
    /// 最后一次 WAL 写入的 LSN；阶段 5 之前写 0。
    pub last_lsn: u64,
}

impl Superblock {
    /// 新建一个 fresh Superblock，magic 与 page_size 由 crate 提供。
    pub fn fresh() -> Self {
        Self {
            magic: PAGE_MAGIC,
            version: 1,
            page_size: PAGE_SIZE as u32,
            free_list_head: None,
            root_page_id: None,
            last_lsn: 0,
        }
    }

    /// 把 Superblock 写入给定的 `Page` 的 user_data。
    ///
    /// 页面自身 header 不变（page_id = 0, page_type = Superblock）。
    pub fn write_into(&self, page: &mut Page) -> Result<(), SpaceError> {
        if page.page_type() != PageType::Superblock {
            return Err(SpaceError::SuperblockTruncated {
                got: page.page_type() as usize,
            });
        }
        let ud = page.user_data_mut();
        if ud.len() < SUPERBLOCK_BYTES {
            return Err(SpaceError::SuperblockTruncated { got: ud.len() });
        }
        ud[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&self.magic.to_le_bytes());
        ud[OFF_VERSION..OFF_VERSION + 4].copy_from_slice(&self.version.to_le_bytes());
        ud[OFF_PAGE_SIZE..OFF_PAGE_SIZE + 4].copy_from_slice(&self.page_size.to_le_bytes());
        match self.free_list_head {
            Some(id) => {
                ud[OFF_FREE_HEAD_SOME] = 1;
                ud[OFF_FREE_HEAD..OFF_FREE_HEAD + 4].copy_from_slice(&id.to_le_bytes());
            }
            None => {
                ud[OFF_FREE_HEAD_SOME] = 0;
                ud[OFF_FREE_HEAD..OFF_FREE_HEAD + 4].copy_from_slice(&[0u8; 4]);
            }
        }
        match self.root_page_id {
            Some(id) => {
                ud[OFF_ROOT_SOME] = 1;
                ud[OFF_ROOT..OFF_ROOT + 4].copy_from_slice(&id.to_le_bytes());
            }
            None => {
                ud[OFF_ROOT_SOME] = 0;
                ud[OFF_ROOT..OFF_ROOT + 4].copy_from_slice(&[0u8; 4]);
            }
        }
        ud[OFF_LAST_LSN..OFF_LAST_LSN + 8].copy_from_slice(&self.last_lsn.to_le_bytes());
        // 剩余字节保持 0。
        Ok(())
    }

    /// 从 `Page` 的 user_data 中读出 Superblock。
    pub fn read_from(page: &Page) -> Result<Self, SpaceError> {
        let ud = page.user_data();
        if ud.len() < SUPERBLOCK_BYTES {
            return Err(SpaceError::SuperblockTruncated { got: ud.len() });
        }
        let magic = u32::from_le_bytes([ud[0], ud[1], ud[2], ud[3]]);
        if magic != PAGE_MAGIC {
            return Err(SpaceError::SuperblockInvalidMagic);
        }
        let version = u32::from_le_bytes([ud[4], ud[5], ud[6], ud[7]]);
        let page_size = u32::from_le_bytes([ud[8], ud[9], ud[10], ud[11]]);
        if page_size != PAGE_SIZE as u32 {
            return Err(SpaceError::SuperblockPageSizeMismatch {
                file: page_size,
                build: PAGE_SIZE as u32,
            });
        }
        let free_some = ud[OFF_FREE_HEAD_SOME] != 0;
        let free_head = if free_some {
            Some(u32::from_le_bytes([
                ud[OFF_FREE_HEAD],
                ud[OFF_FREE_HEAD + 1],
                ud[OFF_FREE_HEAD + 2],
                ud[OFF_FREE_HEAD + 3],
            ]))
        } else {
            None
        };
        let root_some = ud[OFF_ROOT_SOME] != 0;
        let root = if root_some {
            Some(u32::from_le_bytes([
                ud[OFF_ROOT],
                ud[OFF_ROOT + 1],
                ud[OFF_ROOT + 2],
                ud[OFF_ROOT + 3],
            ]))
        } else {
            None
        };
        let last_lsn = u64::from_le_bytes([
            ud[OFF_LAST_LSN],
            ud[OFF_LAST_LSN + 1],
            ud[OFF_LAST_LSN + 2],
            ud[OFF_LAST_LSN + 3],
            ud[OFF_LAST_LSN + 4],
            ud[OFF_LAST_LSN + 5],
            ud[OFF_LAST_LSN + 6],
            ud[OFF_LAST_LSN + 7],
        ]);

        Ok(Self {
            magic,
            version,
            page_size,
            free_list_head: free_head,
            root_page_id: root,
            last_lsn,
        })
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_superblock_round_trip() {
        let sb = Superblock::fresh();
        let mut page = Page::new(0, PageType::Superblock);
        sb.write_into(&mut page).unwrap();
        let decoded = Superblock::read_from(&page).unwrap();
        assert_eq!(sb, decoded);
    }

    #[test]
    fn set_root_persists() {
        let mut sb = Superblock::fresh();
        sb.root_page_id = Some(42);
        sb.free_list_head = Some(7);
        let mut page = Page::new(0, PageType::Superblock);
        sb.write_into(&mut page).unwrap();
        let decoded = Superblock::read_from(&page).unwrap();
        assert_eq!(decoded.root_page_id, Some(42));
        assert_eq!(decoded.free_list_head, Some(7));
    }

    #[test]
    fn bad_magic_rejected() {
        let mut page = Page::new(0, PageType::Superblock);
        // Corrupt the magic in user_data.
        page.user_data_mut()[0] = 0xDE;
        page.user_data_mut()[1] = 0xAD;
        let err = Superblock::read_from(&page);
        assert!(matches!(err, Err(SpaceError::SuperblockInvalidMagic)));
    }
}
