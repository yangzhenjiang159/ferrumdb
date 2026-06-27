//! 16KB 页布局、页头/页尾与用户区。
//!
//! # 字节序
//!
//! 全项目统一 **little-endian**。
//!
//! # 磁盘布局（16384 字节）
//!
//! ```text
//! +---------------------------+  offset 0
//! | PageHeader (32 bytes)     |
//! +---------------------------+  offset 32
//! | User Data (16344 bytes)   |
//! +---------------------------+  offset 16376
//! | PageFooter (8 bytes)      |
//! +---------------------------+  offset 16384
//! ```

use crate::error::PageError;

/// 页大小：16KB，与 InnoDB 默认页大小一致。
pub const PAGE_SIZE: usize = 16384;

/// 页头长度（固定）。
pub const PAGE_HEADER_SIZE: usize = 32;

/// 页尾长度（固定，预留校验冗余）。
pub const PAGE_FOOTER_SIZE: usize = 8;

/// 用户数据区起始偏移。
pub const PAGE_USER_DATA_OFFSET: usize = PAGE_HEADER_SIZE;

/// 页尾起始偏移。
pub const PAGE_FOOTER_OFFSET: usize = PAGE_SIZE - PAGE_FOOTER_SIZE;

/// 用户数据区长度。
pub const PAGE_USER_DATA_SIZE: usize = PAGE_SIZE - PAGE_HEADER_SIZE - PAGE_FOOTER_SIZE;

/// 页文件头 magic，用于识别 FerrumDB 页：`FE DB 00 01`。
pub const PAGE_MAGIC: u32 = 0xFEDB_0001;

/// 当前页头格式版本。
pub const PAGE_HEADER_VERSION: u32 = 1;

mod header_offset {
    pub const MAGIC: usize = 0;
    pub const PAGE_ID: usize = 4;
    pub const PAGE_TYPE: usize = 8;
    pub const LSN: usize = 12;
    pub const CHECKSUM: usize = 20;
    pub const VERSION: usize = 24;
}

mod footer_offset {
    pub const MAGIC: usize = 0;
    pub const CHECKSUM: usize = 4;
}

/// 页类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageType {
    /// 空闲页 / 空闲链表
    Free = 0,
    /// 数据页（Slotted Page，阶段 2）
    Data = 1,
    /// B+Tree 索引节点页
    Index = 2,
    /// 表空间 superblock（第 0 页）
    Superblock = 3,
}

impl PageType {
    /// 转为磁盘上的单字节表示。
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// 从磁盘字节解析。
    pub fn from_u8(value: u8) -> Result<Self, PageError> {
        match value {
            0 => Ok(Self::Free),
            1 => Ok(Self::Data),
            2 => Ok(Self::Index),
            3 => Ok(Self::Superblock),
            other => Err(PageError::UnknownPageType(other)),
        }
    }
}

/// 页头（逻辑视图，与磁盘 32 字节对应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageHeader {
    /// 页在表空间中的逻辑编号。
    pub page_id: u32,
    /// 页类型。
    pub page_type: PageType,
    /// 最后修改该页的 Log Sequence Number（阶段 5 使用）。
    pub lsn: u64,
    /// CRC32 校验和；序列化时写入，由 [`Page::to_bytes`] 计算。
    pub checksum: u32,
}

impl PageHeader {
    /// 构造新页头；`checksum` 初始为 0，序列化时填充。
    pub fn new(page_id: u32, page_type: PageType) -> Self {
        Self {
            page_id,
            page_type,
            lsn: 0,
            checksum: 0,
        }
    }
}

/// 页尾（8 字节，冗余校验）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFooter {
    /// 与页头相同的 magic。
    pub magic: u32,
    /// 与页头相同的 checksum。
    pub checksum: u32,
}

/// 内存中的页：页头 + 用户区 + 页尾。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    header: PageHeader,
    user_data: [u8; PAGE_USER_DATA_SIZE],
    footer: PageFooter,
}

impl Page {
    /// 创建空白页；用户区清零，checksum 尚未计算。
    pub fn new(page_id: u32, page_type: PageType) -> Self {
        Self {
            header: PageHeader::new(page_id, page_type),
            user_data: [0u8; PAGE_USER_DATA_SIZE],
            footer: PageFooter {
                magic: PAGE_MAGIC,
                checksum: 0,
            },
        }
    }

    pub fn header(&self) -> &PageHeader {
        &self.header
    }

    pub fn footer(&self) -> &PageFooter {
        &self.footer
    }

    pub fn page_id(&self) -> u32 {
        self.header.page_id
    }

    pub fn page_type(&self) -> PageType {
        self.header.page_type
    }

    pub fn lsn(&self) -> u64 {
        self.header.lsn
    }

    /// 更新 LSN（修改页内容后由上层调用，阶段 5）。
    pub fn set_lsn(&mut self, lsn: u64) {
        self.header.lsn = lsn;
    }

    pub fn user_data(&self) -> &[u8; PAGE_USER_DATA_SIZE] {
        &self.user_data
    }

    pub fn user_data_mut(&mut self) -> &mut [u8; PAGE_USER_DATA_SIZE] {
        &mut self.user_data
    }

    /// 将页序列化为 16384 字节。
    pub fn to_bytes(&self) -> [u8; PAGE_SIZE] {
        let mut bytes = [0u8; PAGE_SIZE];
        Self::write_header_to(&mut bytes, &self.header, 0);
        bytes[PAGE_USER_DATA_OFFSET..PAGE_FOOTER_OFFSET]
            .copy_from_slice(&self.user_data);
        Self::write_footer_to(&mut bytes, PAGE_MAGIC, 0);

        let checksum = Self::compute_checksum(&bytes);
        Self::write_header_to(&mut bytes, &self.header, checksum);
        Self::write_footer_to(&mut bytes, PAGE_MAGIC, checksum);
        bytes
    }

    /// 从字节切片解析页；长度必须为 [`PAGE_SIZE`]。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PageError> {
        let bytes: &[u8; PAGE_SIZE] = bytes
            .try_into()
            .map_err(|_| PageError::InvalidLength(bytes.len()))?;

        Self::verify_magic(bytes)?;
        Self::verify_checksum(bytes)?;

        let header = Self::parse_header(bytes)?;
        let footer = Self::parse_footer(bytes)?;

        let mut user_data = [0u8; PAGE_USER_DATA_SIZE];
        user_data.copy_from_slice(&bytes[PAGE_USER_DATA_OFFSET..PAGE_FOOTER_OFFSET]);

        Ok(Self {
            header,
            user_data,
            footer,
        })
    }

    /// 计算整页 CRC32；计算时 header/footer 的 checksum 字段视为 0。
    pub fn compute_checksum(page_bytes: &[u8; PAGE_SIZE]) -> u32 {
        let mut copy = *page_bytes;
        copy[header_offset::CHECKSUM..header_offset::CHECKSUM + 4].fill(0);
        copy[PAGE_FOOTER_OFFSET + footer_offset::CHECKSUM
            ..PAGE_FOOTER_OFFSET + footer_offset::CHECKSUM + 4]
            .fill(0);
        crc32fast::hash(&copy)
    }

    /// 校验 embedded checksum 是否与重新计算结果一致。
    pub fn verify_checksum(page_bytes: &[u8; PAGE_SIZE]) -> Result<(), PageError> {
        let expected = Self::compute_checksum(page_bytes);
        let header_checksum = read_u32(page_bytes, header_offset::CHECKSUM);
        let footer_checksum = read_u32(page_bytes, PAGE_FOOTER_OFFSET + footer_offset::CHECKSUM);

        if expected != header_checksum || expected != footer_checksum {
            return Err(PageError::ChecksumMismatch);
        }
        Ok(())
    }

    fn verify_magic(page_bytes: &[u8; PAGE_SIZE]) -> Result<(), PageError> {
        let header_magic = read_u32(page_bytes, header_offset::MAGIC);
        let footer_magic = read_u32(page_bytes, PAGE_FOOTER_OFFSET + footer_offset::MAGIC);
        if header_magic != PAGE_MAGIC || footer_magic != PAGE_MAGIC {
            return Err(PageError::InvalidMagic);
        }
        Ok(())
    }

    fn write_header_to(buf: &mut [u8; PAGE_SIZE], header: &PageHeader, checksum: u32) {
        write_u32(buf, header_offset::MAGIC, PAGE_MAGIC);
        write_u32(buf, header_offset::PAGE_ID, header.page_id);
        buf[header_offset::PAGE_TYPE] = header.page_type.to_u8();
        write_u64(buf, header_offset::LSN, header.lsn);
        write_u32(buf, header_offset::CHECKSUM, checksum);
        write_u32(buf, header_offset::VERSION, PAGE_HEADER_VERSION);
    }

    fn write_footer_to(buf: &mut [u8; PAGE_SIZE], magic: u32, checksum: u32) {
        write_u32(buf, PAGE_FOOTER_OFFSET + footer_offset::MAGIC, magic);
        write_u32(
            buf,
            PAGE_FOOTER_OFFSET + footer_offset::CHECKSUM,
            checksum,
        );
    }

    fn parse_header(buf: &[u8; PAGE_SIZE]) -> Result<PageHeader, PageError> {
        let page_type = PageType::from_u8(buf[header_offset::PAGE_TYPE])?;
        Ok(PageHeader {
            page_id: read_u32(buf, header_offset::PAGE_ID),
            page_type,
            lsn: read_u64(buf, header_offset::LSN),
            checksum: read_u32(buf, header_offset::CHECKSUM),
        })
    }

    fn parse_footer(buf: &[u8; PAGE_SIZE]) -> Result<PageFooter, PageError> {
        Ok(PageFooter {
            magic: read_u32(buf, PAGE_FOOTER_OFFSET + footer_offset::MAGIC),
            checksum: read_u32(buf, PAGE_FOOTER_OFFSET + footer_offset::CHECKSUM),
        })
    }
}

fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buf[offset..offset + 4].try_into().expect("u32 slice"))
}

fn read_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(buf[offset..offset + 8].try_into().expect("u64 slice"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_new_has_expected_layout_constants() {
        assert_eq!(PAGE_SIZE, 16384);
        assert_eq!(PAGE_USER_DATA_SIZE, 16344);
        assert_eq!(PAGE_FOOTER_OFFSET, 16376);
        assert_eq!(PAGE_HEADER_SIZE + PAGE_USER_DATA_SIZE + PAGE_FOOTER_SIZE, PAGE_SIZE);
    }

    #[test]
    fn page_new_initializes_header() {
        let page = Page::new(42, PageType::Data);
        assert_eq!(page.page_id(), 42);
        assert_eq!(page.page_type(), PageType::Data);
        assert_eq!(page.lsn(), 0);
        assert!(page.user_data().iter().all(|&b| b == 0));
    }

    #[test]
    fn page_round_trip() {
        let mut page = Page::new(1, PageType::Index);
        page.set_lsn(99);
        page.user_data_mut()[0] = 0xAB;
        page.user_data_mut()[PAGE_USER_DATA_SIZE - 1] = 0xCD;

        let bytes = page.to_bytes();
        assert_eq!(bytes.len(), PAGE_SIZE);

        let decoded = Page::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.page_id(), page.page_id());
        assert_eq!(decoded.page_type(), page.page_type());
        assert_eq!(decoded.lsn(), page.lsn());
        assert_eq!(decoded.user_data(), page.user_data());
        assert_ne!(decoded.header().checksum, 0);
        assert_eq!(decoded.footer().magic, PAGE_MAGIC);
        assert_eq!(decoded.footer().checksum, decoded.header().checksum);
    }

    #[test]
    fn page_checksum_detects_corruption() {
        let page = Page::new(1, PageType::Data);
        let mut bytes = page.to_bytes();
        bytes[100] ^= 0xFF;
        assert_eq!(Page::from_bytes(&bytes), Err(PageError::ChecksumMismatch));
    }

    #[test]
    fn page_invalid_length() {
        assert_eq!(
            Page::from_bytes(&[0u8; 100]),
            Err(PageError::InvalidLength(100))
        );
    }

    #[test]
    fn page_invalid_magic() {
        let page = Page::new(1, PageType::Data);
        let mut bytes = page.to_bytes();
        bytes[0] ^= 0xFF;
        assert_eq!(Page::from_bytes(&bytes), Err(PageError::InvalidMagic));
    }

    #[test]
    fn page_type_round_trip() {
        for ty in [
            PageType::Free,
            PageType::Data,
            PageType::Index,
            PageType::Superblock,
        ] {
            assert_eq!(PageType::from_u8(ty.to_u8()).unwrap(), ty);
        }
        assert_eq!(PageType::from_u8(99), Err(PageError::UnknownPageType(99)));
    }
}
