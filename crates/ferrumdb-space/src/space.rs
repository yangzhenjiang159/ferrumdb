//! 表空间主结构：`Space` 持有一个表空间文件 + 内存中的 superblock。

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[cfg(test)]
use ferrumdb_page::PAGE_MAGIC;
use ferrumdb_page::{Page, PageType, PAGE_SIZE};

use crate::error::SpaceError;
use crate::free_list::{decode_free, encode_free};
use crate::superblock::Superblock;

/// 表空间（一个 `tablespace.ibd` 文件）。
pub struct Space {
    path: PathBuf,
    file: File,
    superblock: Superblock,
    /// 文件当前包含的页数。
    page_count: u32,
}

impl Space {
    /// 创建一个新的空表空间文件。如果文件已存在会被截断。
    pub fn create(path: impl AsRef<Path>) -> Result<Self, SpaceError> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len(0)?;

        let sb = Superblock::fresh();
        let page0 = Self::build_page0(&sb)?;
        let bytes = page0.to_bytes();

        file.seek(SeekFrom::Start(0))?;
        file.write_all(&bytes)?;
        file.sync_all()?;

        Ok(Self {
            path,
            file,
            superblock: sb,
            page_count: 1,
        })
    }

    /// 打开一个已存在的表空间文件。校验 superblock。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SpaceError> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Err(SpaceError::SuperblockInvalidMagic);
        }
        if len % PAGE_SIZE as u64 != 0 {
            return Err(SpaceError::SuperblockTruncated { got: len as usize });
        }
        let page_count = (len / PAGE_SIZE as u64) as u32;
        if page_count == 0 {
            return Err(SpaceError::SuperblockInvalidMagic);
        }

        // Read page 0.
        let mut buf = [0u8; PAGE_SIZE];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut buf)?;
        let page0 = Page::from_bytes(&buf).map_err(|_| SpaceError::SuperblockInvalidMagic)?;
        let sb = Superblock::read_from(&page0)?;
        if sb.version > 1 {
            return Err(SpaceError::SuperblockVersionUnsupported(sb.version));
        }

        Ok(Self {
            path,
            file,
            superblock: sb,
            page_count,
        })
    }

    fn build_page0(sb: &Superblock) -> Result<Page, SpaceError> {
        let mut page = Page::new(0, PageType::Superblock);
        sb.write_into(&mut page)?;
        let bytes = page.to_bytes();
        Page::from_bytes(&bytes).map_err(|_| SpaceError::SuperblockInvalidMagic)
    }

    /// 强制 fsync。
    pub fn sync_all(&mut self) -> Result<(), SpaceError> {
        self.file.sync_all()?;
        Ok(())
    }

    /// 表空间文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 当前文件包含的页数。
    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    /// 当前 superblock 的引用。
    pub fn superblock(&self) -> &Superblock {
        &self.superblock
    }

    /// 设置 root page id 并立即 fsync。
    pub fn set_root_page_id(&mut self, page_id: u32) -> Result<(), SpaceError> {
        self.superblock.root_page_id = Some(page_id);
        self.write_superblock()
    }

    fn write_superblock(&mut self) -> Result<(), SpaceError> {
        let page0 = Self::build_page0(&self.superblock)?;
        let bytes = page0.to_bytes();
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        Ok(())
    }

    fn offset_of(page_id: u32) -> u64 {
        page_id as u64 * PAGE_SIZE as u64
    }

    /// 读取一个页。
    pub fn read_page(&mut self, page_id: u32) -> Result<Page, SpaceError> {
        if page_id >= self.page_count {
            return Err(SpaceError::PageIdOutOfRange(page_id));
        }
        let mut buf = vec![0u8; PAGE_SIZE];
        self.file.seek(SeekFrom::Start(Self::offset_of(page_id)))?;
        self.file.read_exact(&mut buf)?;
        let page = Page::from_bytes(&buf).map_err(|_| SpaceError::SuperblockInvalidMagic)?;
        Ok(page)
    }

    /// 写入一个页并 fsync。
    pub fn write_page(&mut self, page_id: u32, page: &Page) -> Result<(), SpaceError> {
        if page_id >= self.page_count {
            return Err(SpaceError::PageIdOutOfRange(page_id));
        }
        let bytes = page.to_bytes();
        self.file.seek(SeekFrom::Start(Self::offset_of(page_id)))?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        Ok(())
    }

    /// 分配一个新页（优先从 free list 取，否则扩展文件）。
    ///
    /// 返回的 PageId 对应一个全零 `PageType::Free` 页；调用方负责用 `write_page`
    /// 覆盖为期望的类型与内容。
    pub fn allocate_page(&mut self) -> Result<u32, SpaceError> {
        if let Some(head_id) = self.superblock.free_list_head {
            // Pop head from free list.
            let mut page = self.read_page(head_id)?;
            let next = decode_free(page.user_data())?;
            // Validate next is within range or None.
            if let Some(n) = next {
                if n >= self.page_count {
                    return Err(SpaceError::FreeListCorrupted(head_id));
                }
            }
            // Update superblock.
            self.superblock.free_list_head = next;
            self.write_superblock()?;
            // Zero out the freed page's user_data (preserve header + checksum).
            for b in page.user_data_mut().iter_mut() {
                *b = 0;
            }
            self.write_page(head_id, &page)?;
            Ok(head_id)
        } else {
            // Extend file by one page.
            let new_id = self.page_count;
            let new_len = (self.page_count as u64 + 1) * PAGE_SIZE as u64;
            self.file.set_len(new_len)?;
            // Bump page_count BEFORE write_page so the page_id range check passes.
            self.page_count += 1;
            // Initialize the new page as a zero Free page.
            let mut new_page = Page::new(new_id, PageType::Free);
            for b in new_page.user_data_mut().iter_mut() {
                *b = 0;
            }
            self.write_page(new_id, &new_page)?;
            // Re-write superblock to persist any state change.
            self.write_superblock()?;
            Ok(new_id)
        }
    }

    /// 把页放回 free list。
    ///
    /// 页内容会被标记为 `PageType::Free` 并写入 next 指针；superblock 的
    /// `free_list_head` 更新为该 id 并 fsync。
    pub fn free_page(&mut self, page_id: u32) -> Result<(), SpaceError> {
        if page_id == 0 {
            return Err(SpaceError::PageIdOutOfRange(page_id)); // page 0 is superblock
        }
        if page_id >= self.page_count {
            return Err(SpaceError::PageIdOutOfRange(page_id));
        }
        let mut page = Page::new(page_id, PageType::Free);
        let encoded = encode_free(self.superblock.free_list_head);
        page.user_data_mut()[..encoded.len()].copy_from_slice(&encoded);
        self.write_page(page_id, &page)?;
        self.superblock.free_list_head = Some(page_id);
        self.write_superblock()?;
        Ok(())
    }
}

// `to_bytes` / `from_bytes` for Page already include PAGE_MAGIC in the header.

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path() -> std::path::PathBuf {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("test.ibd");
        // Leak the TempDir to keep the file alive for the test; cleaned up when process exits.
        Box::leak(Box::new(dir));
        p
    }

    #[test]
    fn create_then_open_round_trip() {
        let path = tmp_path();
        {
            let space = Space::create(&path).unwrap();
            assert_eq!(space.page_count(), 1);
            assert_eq!(space.superblock().magic, PAGE_MAGIC);
        }
        let space = Space::open(&path).unwrap();
        assert_eq!(space.page_count(), 1);
        assert_eq!(space.superblock().magic, PAGE_MAGIC);
        assert_eq!(space.superblock().version, 1);
    }

    #[test]
    fn open_bad_magic_returns_error() {
        let path = tmp_path();
        // Create a 16KB file with all zeros (bad page magic).
        std::fs::write(&path, vec![0u8; PAGE_SIZE]).unwrap();
        let err = Space::open(&path);
        assert!(matches!(err, Err(SpaceError::SuperblockInvalidMagic)));
    }

    #[test]
    fn allocate_extends_file() {
        let path = tmp_path();
        let mut space = Space::create(&path).unwrap();
        assert_eq!(space.page_count(), 1);
        let id = space.allocate_page().unwrap();
        assert_eq!(id, 1);
        assert_eq!(space.page_count(), 2);
        // File should now be 2 * PAGE_SIZE bytes.
        let len = std::fs::metadata(&path).unwrap().len();
        assert_eq!(len, PAGE_SIZE as u64 * 2);
    }

    #[test]
    fn alloc_then_free_then_alloc_returns_same_id() {
        let path = tmp_path();
        let mut space = Space::create(&path).unwrap();
        let id1 = space.allocate_page().unwrap();
        assert_eq!(id1, 1);
        space.free_page(id1).unwrap();
        let id2 = space.allocate_page().unwrap();
        assert_eq!(id2, id1);
    }

    #[test]
    fn free_list_chains_correctly() {
        let path = tmp_path();
        let mut space = Space::create(&path).unwrap();
        let _id1 = space.allocate_page().unwrap(); // 1
        let id2 = space.allocate_page().unwrap(); // 2
        let id3 = space.allocate_page().unwrap(); // 3
        space.free_page(id2).unwrap(); // head -> 2 -> None
        space.free_page(id3).unwrap(); // head -> 3 -> 2 -> None
        // Re-allocate: should get 3, then 2, then 4 (extend)
        assert_eq!(space.allocate_page().unwrap(), id3);
        assert_eq!(space.allocate_page().unwrap(), id2);
        assert_eq!(space.allocate_page().unwrap(), 4);
    }

    #[test]
    fn write_and_read_page() {
        let path = tmp_path();
        let mut space = Space::create(&path).unwrap();
        let id = space.allocate_page().unwrap();
        let mut page = Page::new(id, PageType::Data);
        page.user_data_mut()[0] = 0xAB;
        page.user_data_mut()[1] = 0xCD;
        space.write_page(id, &page).unwrap();

        let read = space.read_page(id).unwrap();
        assert_eq!(read.user_data()[0], 0xAB);
        assert_eq!(read.user_data()[1], 0xCD);
    }

    #[test]
    fn set_root_persists() {
        let path = tmp_path();
        {
            let mut space = Space::create(&path).unwrap();
            space.set_root_page_id(42).unwrap();
        }
        let space = Space::open(&path).unwrap();
        assert_eq!(space.superblock().root_page_id, Some(42));
    }
}
