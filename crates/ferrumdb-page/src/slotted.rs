//! Slotted Page：用户数据区内的行槽位管理。
//!
//! # 布局（在 [`Page::user_data`] 内部）
//!
//! ```text
//! +---------------------------+  offset 0
//! | header: free_offset (u16) |
//! | header: free_upper  (u16) |
//! | header: slot_count  (u16) |  <- 共 6 字节
//! +---------------------------+  offset 6
//! | record 0 bytes            |
//! | record 1 bytes            |
//! | ...                       |
//! | (free space)              |
//! | ...                       |
//! | record N-1 bytes          |
//! +---------------------------+
//! | slot N-1: (off, len) u16x2|  4 bytes/entry
//! | slot N-2: (off, len)      |
//! | ...                       |
//! | slot 0: (off, len)        |
//! +---------------------------+  offset PAGE_USER_DATA_SIZE
//! ```
//!
//! 记录从前往后增长，slot 目录从后往前增长。`Page` 自身的 32-byte header 与 8-byte footer
//! 不变，Slotted Page 仅占据中间的 `PAGE_USER_DATA_SIZE` 字节。
//!
//! 删除 = 把对应 slot 标记为 `(0, 0)`（tombstone）；新插入可重用 tombstone 位置。
//!
//! 见项目文档 `docs/plan.md` 阶段 2。

use crate::error::PageError;
use crate::page::{Page, PageType, PAGE_USER_DATA_SIZE};

const HEADER_BYTES: usize = 6; // free_offset + free_upper + slot_count, all u16 LE
const SLOT_BYTES: usize = 4;   // (offset: u16, len: u16)

/// 单个槽位条目：记录在用户数据区中的偏移与长度。
///
/// `offset == 0 && len == 0` 表示该 slot 已删除（tombstone），后续 `insert` 可重用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotEntry {
    /// 记录在 `user_data` 中的起始偏移（含 6 字节 SlottedPage 头部）。
    pub offset: u16,
    /// 记录字节长度；0 表示 tombstone。
    pub len: u16,
}

/// 内存中的 Slotted Page。
///
/// 序列化进 [`Page::user_data`]；不修改 [`Page`] 自身的 32-byte header / 8-byte footer。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlottedPage {
    page_id: u32,
    page_type: PageType,
    slots: Vec<SlotEntry>,
    free_offset: u16,
    free_upper: u16,
    /// 内存中保存所有记录的字节缓冲（`user_data[HEADER_BYTES..free_offset]` 是有效区）。
    /// 比 `Vec<Vec<u8>>` 更紧凑，便于直接序列化到 `Page`。
    records: Vec<u8>,
}

impl SlottedPage {
    /// 创建一个空 Slotted Page，slot 目录为空。
    pub fn new(page_id: u32, page_type: PageType) -> Self {
        Self {
            page_id,
            page_type,
            slots: Vec::new(),
            free_offset: HEADER_BYTES as u16,
            free_upper: PAGE_USER_DATA_SIZE as u16,
            records: Vec::new(),
        }
    }

    pub fn page_id(&self) -> u32 {
        self.page_id
    }

    pub fn page_type(&self) -> PageType {
        self.page_type
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn free_space(&self) -> usize {
        // available bytes = free_upper - free_offset (records can grow here)
        (self.free_upper - self.free_offset) as usize
    }

    /// 在指定 `slot_id` 写入或覆盖一条记录。
    ///
    /// - `slot_id < self.slots.len()`：覆盖该 slot 的记录（即使之前是 tombstone）
    /// - `slot_id == self.slots.len()`：追加新 slot
    /// - `slot_id > self.slots.len()`：返回 [`PageError::SlotOutOfRange`]
    ///
    /// # Errors
    ///
    /// - 用户区无足够空间 ⇒ [`PageError::PageFull`]
    /// - `slot_id` 越界 ⇒ [`PageError::SlotOutOfRange`]
    pub fn insert(&mut self, slot_id: usize, record: &[u8]) -> Result<(), PageError> {
        if record.len() > u16::MAX as usize {
            return Err(PageError::EncodingError(format!(
                "record length {} exceeds u16::MAX",
                record.len()
            )));
        }
        if slot_id > self.slots.len() {
            return Err(PageError::SlotOutOfRange(slot_id));
        }

        // Case 1: existing slot, same size → in-place rewrite at the same byte range.
        if slot_id < self.slots.len() {
            let existing_len = self.slots[slot_id].len as usize;
            if existing_len == record.len() && existing_len > 0 {
                let ud_offset = self.slots[slot_id].offset as usize;
                if ud_offset >= HEADER_BYTES {
                    let rec_offset = ud_offset - HEADER_BYTES;
                    let end = rec_offset + record.len();
                    if end <= self.records.len() {
                        self.records[rec_offset..end].copy_from_slice(record);
                        return Ok(());
                    }
                }
            }
        }

        // Case 2: append new slot. Reserve record + slot entry.
        let needed = record.len() + SLOT_BYTES;
        let available = (self.free_upper - self.free_offset) as usize;
        if available < needed {
            return Err(PageError::PageFull);
        }

        // self.records holds ONLY record bytes (no header); offset is 0-based in that buffer.
        // self.free_offset is the corresponding position in user_data (includes HEADER_BYTES).
        let rec_offset = self.records.len();
        let new_offset = HEADER_BYTES + rec_offset; // offset within user_data
        self.records.extend_from_slice(record);

        let new_slot = SlotEntry {
            offset: new_offset as u16,
            len: record.len() as u16,
        };
        self.slots.push(new_slot);
        self.free_upper -= SLOT_BYTES as u16;
        self.free_offset = (new_offset + record.len()) as u16;
        Ok(())
    }

    /// 读取 `slot_id` 对应的记录。
    pub fn get(&self, slot_id: usize) -> Option<&[u8]> {
        let slot = *self.slots.get(slot_id)?;
        if slot.len == 0 {
            return None; // tombstone
        }
        // slot.offset is the offset within `user_data` (includes the 6-byte header).
        // self.records only holds the actual record bytes, so subtract the header size.
        let start = slot.offset as usize;
        if start < HEADER_BYTES {
            return None;
        }
        let rec_start = start - HEADER_BYTES;
        let rec_end = rec_start + slot.len as usize;
        if rec_end > self.records.len() {
            return None;
        }
        Some(&self.records[rec_start..rec_end])
    }

    /// 删除 `slot_id` 对应的记录（标记 tombstone）。
    pub fn delete(&mut self, slot_id: usize) -> Result<(), PageError> {
        if slot_id >= self.slots.len() {
            return Err(PageError::SlotOutOfRange(slot_id));
        }
        self.slots[slot_id].len = 0;
        self.slots[slot_id].offset = 0;
        Ok(())
    }

    /// 序列化为可以放入 [`Page::user_data`] 的字节序列。
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![0u8; PAGE_USER_DATA_SIZE];
        // Header: free_offset (u16) | free_upper (u16) | slot_count (u16)
        out[0..2].copy_from_slice(&self.free_offset.to_le_bytes());
        out[2..4].copy_from_slice(&self.free_upper.to_le_bytes());
        out[4..6].copy_from_slice(&(self.slots.len() as u16).to_le_bytes());
        // Records area: [HEADER_BYTES .. free_offset]
        // self.records holds only the record bytes (no header).
        let rec_end = HEADER_BYTES + self.records.len();
        let target_end = (self.free_offset as usize).min(PAGE_USER_DATA_SIZE);
        if rec_end <= target_end {
            out[HEADER_BYTES..rec_end].copy_from_slice(&self.records);
        } else {
            // free_offset should equal HEADER_BYTES + records.len() by invariant;
            // fall back to a safe partial copy.
            let safe_end = target_end.min(rec_end).min(HEADER_BYTES + self.records.len());
            out[HEADER_BYTES..safe_end].copy_from_slice(&self.records[..safe_end - HEADER_BYTES]);
        }
        // Slot directory grows down from PAGE_USER_DATA_SIZE.
        let mut dir_pos = PAGE_USER_DATA_SIZE;
        for slot in self.slots.iter().rev() {
            dir_pos -= SLOT_BYTES;
            out[dir_pos..dir_pos + 2].copy_from_slice(&slot.offset.to_le_bytes());
            out[dir_pos + 2..dir_pos + 4].copy_from_slice(&slot.len.to_le_bytes());
        }
        out
    }

    /// 从 [`Page`] 反序列化 SlottedPage。
    pub fn from_page(page: &Page) -> Result<Self, PageError> {
        if page.page_type() != PageType::Data && page.page_type() != PageType::Index {
            return Err(PageError::EncodingError(format!(
                "SlottedPage requires Data or Index page type, got {:?}",
                page.page_type()
            )));
        }
        let ud = page.user_data();
        if ud.len() < HEADER_BYTES {
            return Err(PageError::EncodingError(
                "user_data shorter than SlottedPage header".into(),
            ));
        }
        let free_offset = u16::from_le_bytes([ud[0], ud[1]]);
        let free_upper = u16::from_le_bytes([ud[2], ud[3]]);
        let slot_count = u16::from_le_bytes([ud[4], ud[5]]) as usize;

        if free_upper < free_offset {
            return Err(PageError::EncodingError("free_upper < free_offset".into()));
        }
        let available = (free_upper - free_offset) as usize;
        if available < slot_count * SLOT_BYTES {
            return Err(PageError::EncodingError(
                "slot directory larger than free space".into(),
            ));
        }
        if free_upper as usize > PAGE_USER_DATA_SIZE {
            return Err(PageError::EncodingError("free_upper out of bounds".into()));
        }

        let mut slots = Vec::with_capacity(slot_count);
        let mut dir_pos = PAGE_USER_DATA_SIZE;
        for _ in 0..slot_count {
            dir_pos -= SLOT_BYTES;
            let offset = u16::from_le_bytes([ud[dir_pos], ud[dir_pos + 1]]);
            let len = u16::from_le_bytes([ud[dir_pos + 2], ud[dir_pos + 3]]);
            slots.push(SlotEntry { offset, len });
        }
        slots.reverse();

        // Records area: ud[HEADER_BYTES .. free_offset] — only the actual record bytes,
        // not the SlottedPage header. The header is read separately above.
        let mut records = Vec::with_capacity(free_offset.saturating_sub(HEADER_BYTES as u16) as usize);
        if free_offset as usize > HEADER_BYTES {
            records.extend_from_slice(&ud[HEADER_BYTES..free_offset as usize]);
        }

        Ok(Self {
            page_id: page.page_id(),
            page_type: page.page_type(),
            slots,
            free_offset,
            free_upper,
            records,
        })
    }

    /// 与 [`SlottedPage::to_bytes`] + [`SlottedPage::from_page`] 的便捷组合。
    pub fn round_trip(&self) -> Result<SlottedPage, PageError> {
        let bytes = self.to_bytes();
        let mut page = Page::new(self.page_id, self.page_type);
        page.user_data_mut()[..bytes.len()].copy_from_slice(&bytes);
        SlottedPage::from_page(&page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_page_round_trip() {
        let sp = SlottedPage::new(7, PageType::Data);
        let rt = sp.round_trip().unwrap();
        assert_eq!(sp, rt);
        assert_eq!(rt.slot_count(), 0);
    }

    #[test]
    fn insert_and_get_record() {
        let mut sp = SlottedPage::new(1, PageType::Data);
        sp.insert(0, b"hello").unwrap();
        sp.insert(1, b"world").unwrap();
        assert_eq!(sp.get(0), Some(&b"hello"[..]));
        assert_eq!(sp.get(1), Some(&b"world"[..]));
        assert_eq!(sp.slot_count(), 2);

        let rt = sp.round_trip().unwrap();
        assert_eq!(rt.get(0), Some(&b"hello"[..]));
        assert_eq!(rt.get(1), Some(&b"world"[..]));
    }

    #[test]
    fn overwrite_existing_slot() {
        let mut sp = SlottedPage::new(1, PageType::Data);
        sp.insert(0, b"abc").unwrap();
        sp.insert(0, b"xyz").unwrap();
        assert_eq!(sp.get(0), Some(&b"xyz"[..]));
        assert_eq!(sp.slot_count(), 1);
    }

    #[test]
    fn delete_creates_tombstone() {
        let mut sp = SlottedPage::new(1, PageType::Data);
        sp.insert(0, b"a").unwrap();
        sp.insert(1, b"b").unwrap();
        sp.delete(0).unwrap();
        assert_eq!(sp.get(0), None);
        assert_eq!(sp.get(1), Some(&b"b"[..]));

        let rt = sp.round_trip().unwrap();
        assert_eq!(rt.get(0), None);
        assert_eq!(rt.get(1), Some(&b"b"[..]));
    }

    #[test]
    fn page_full_returns_error() {
        let mut sp = SlottedPage::new(1, PageType::Data);
        // PAGE_USER_DATA_SIZE = 16344; requesting a record of that size must fail.
        let huge = vec![0u8; PAGE_USER_DATA_SIZE];
        let err = sp.insert(0, &huge);
        assert!(matches!(err, Err(PageError::PageFull)));
    }

    #[test]
    fn slot_out_of_range() {
        let mut sp = SlottedPage::new(1, PageType::Data);
        let err = sp.insert(5, b"x");
        assert!(matches!(err, Err(PageError::SlotOutOfRange(5))));
    }

    #[test]
    fn many_records_with_growth() {
        let mut sp = SlottedPage::new(1, PageType::Index);
        for i in 0..100u32 {
            let bytes = (i as u64).to_le_bytes();
            sp.insert(i as usize, &bytes).unwrap();
        }
        assert_eq!(sp.slot_count(), 100);
        let rt = sp.round_trip().unwrap();
        for i in 0..100u32 {
            let bytes = (i as u64).to_le_bytes();
            assert_eq!(rt.get(i as usize), Some(&bytes[..]));
        }
    }

    #[test]
    fn wrong_page_type_rejected() {
        let page = Page::new(1, PageType::Free);
        let err = SlottedPage::from_page(&page);
        assert!(matches!(err, Err(PageError::EncodingError(_))));
    }

    #[test]
    fn truncate_then_get_returns_none() {
        let mut sp = SlottedPage::new(1, PageType::Data);
        sp.insert(0, b"alpha").unwrap();
        sp.delete(0).unwrap();
        assert_eq!(sp.get(0), None);
        // Round-trip still tombstone.
        let rt = sp.round_trip().unwrap();
        assert_eq!(rt.get(0), None);
    }
}
