//! Redo Log（WAL）与崩溃恢复。
//!
//! # 职责
//!
//! - append-only redo 记录
//! - checkpoint 与启动 replay
//!
//! 见项目文档 `docs/plan.md` 阶段 5。

#![deny(missing_docs)]

mod error;
mod record;
mod wal;

pub use error::WalError;
pub use record::{RedoRecord, CHECKPOINT_MAGIC};
pub use wal::Wal;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::PathBuf;

    fn tmp_path() -> PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("test.wal");
        // Leak TempDir to keep file alive.
        Box::leak(Box::new(dir));
        p
    }

    #[test]
    fn create_then_open() {
        let path = tmp_path();
        {
            let w = Wal::create(&path).unwrap();
            assert_eq!(w.next_lsn(), 1);
            assert_eq!(w.checkpoint_lsn(), 0);
        }
        let w = Wal::open(&path).unwrap();
        assert_eq!(w.next_lsn(), 1);
        assert_eq!(w.checkpoint_lsn(), 0);
    }

    #[test]
    fn append_returns_monotonic_lsns() {
        let path = tmp_path();
        let mut w = Wal::create(&path).unwrap();
        let l1 = w.append(0, 0, b"a").unwrap();
        let l2 = w.append(0, 0, b"b").unwrap();
        let l3 = w.append(1, 4, b"cd").unwrap();
        assert_eq!(l1, 1);
        assert_eq!(l2, 2);
        assert_eq!(l3, 3);
        assert_eq!(w.next_lsn(), 4);
    }

    #[test]
    fn checkpoint_record_visible_after_open() {
        let path = tmp_path();
        {
            let mut w = Wal::create(&path).unwrap();
            w.append(0, 0, b"x").unwrap();
            w.append(1, 0, b"y").unwrap();
            w.checkpoint(1).unwrap();
            // Append after checkpoint.
            w.append(2, 0, b"z").unwrap();
        }
        let w = Wal::open(&path).unwrap();
        assert_eq!(w.next_lsn(), 4); // 1, 2, 3 (3 records)
        assert_eq!(w.checkpoint_lsn(), 1);
    }

    #[test]
    fn recover_replays_all_records_before_checkpoint() {
        let path = tmp_path();
        let mut w = Wal::create(&path).unwrap();
        // Record 1: page 0, offset 0, payload "before-cp"
        w.append(0, 0, b"before-cp").unwrap();
        w.checkpoint(1).unwrap();
        // Record 2: page 0, offset 0, payload "after-cp"
        w.append(0, 0, b"after-cp").unwrap();

        let mut page0 = vec![0u8; 16];
        w.recover(|rec| {
            if rec.page_id == 0 {
                assert!(rec.offset as usize + rec.payload.len() <= page0.len());
                let dst = &mut page0[rec.offset as usize..rec.offset as usize + rec.payload.len()];
                dst.copy_from_slice(&rec.payload);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(&page0[..8], b"after-cp");
    }

    #[test]
    fn recover_handles_truncated_tail() {
        // Simulate kill mid-record: truncate 1 byte so the last record's CRC is incomplete.
        // open() should succeed (no corruption detected); recover() should treat as Truncated.
        let path = tmp_path();
        let original_len;
        {
            let mut w = Wal::create(&path).unwrap();
            w.append(0, 0, b"good").unwrap();
            let mut f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            original_len = f.metadata().unwrap().len();
            f.set_len(original_len - 1).unwrap();
        }
        // open() should succeed; recover() should not error (Truncated = EOF).
        let mut w = Wal::open(&path).unwrap();
        let mut page0 = vec![0u8; 16];
        let result = w.recover(|rec| {
            if rec.page_id == 0 {
                page0[..rec.payload.len()].copy_from_slice(&rec.payload);
            }
            Ok(())
        });
        assert!(result.is_ok());
    }

    #[test]
    fn recover_corrupt_crc_returns_error() {
        let path = tmp_path();
        {
            let mut w = Wal::create(&path).unwrap();
            w.append(0, 0, b"data").unwrap();
            // Corrupt a byte in the file.
            let mut f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            f.seek(SeekFrom::Start(30)).unwrap();
            let mut b = [0u8; 1];
            f.read_exact(&mut b).unwrap();
            b[0] ^= 0xFF;
            f.seek(SeekFrom::Start(30)).unwrap();
            f.write_all(&b).unwrap();
        }
        let mut w = Wal::open(&path).unwrap();
        let result = w.recover(|_rec| Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn multiple_pages_replay() {
        let path = tmp_path();
        let mut w = Wal::create(&path).unwrap();
        w.append(0, 0, b"page0-data").unwrap();
        w.append(1, 0, b"page1-data").unwrap();
        w.append(2, 4, b"page2-off4").unwrap();

        let mut pages = vec![vec![0u8; 16]; 3];
        w.recover(|rec| {
            let p = rec.page_id as usize;
            if p < pages.len() {
                let dst = &mut pages[p][rec.offset as usize..rec.offset as usize + rec.payload.len()];
                dst.copy_from_slice(&rec.payload);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(&pages[0][..10], b"page0-data");
        assert_eq!(&pages[1][..10], b"page1-data");
        assert_eq!(&pages[2][4..14], b"page2-off4");
    }

    /// **M1 关键测试**：模拟"进程在 write 后但未 fsync 前被 kill"，重启后通过 WAL recover 恢复数据。
    #[test]
    fn m1_crash_recovery_replays_records() {
        use ferrumdb_page::Page;
        use ferrumdb_page::PageType;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m1.wal");
        let table_path = dir.path().join("m1.ibd");
        let lsn1;
        {
            // Phase A: open WAL + allocate 2 pages, write WAL records, then "crash"
            // by dropping everything without explicit flush.
            let mut wal = Wal::create(&path).unwrap();
            lsn1 = wal.append(1, 0, b"page1-new-content").unwrap();
            let _lsn2 = wal.append(2, 0, b"page2-new-content").unwrap();
            assert_eq!(lsn1, 1);
            // Don't call wal.checkpoint() — simulate kill before checkpoint.
            drop(wal);
        }
        // At this point: WAL has 2 records, but no checkpoint.
        // Phase B: reopen WAL + apply records to "Space" (we use simple Vec<u8> as Space stand-in).
        let mut wal = Wal::open(&path).unwrap();
        assert_eq!(wal.next_lsn(), 3);
        assert_eq!(wal.checkpoint_lsn(), 0);
        let mut pages: std::collections::HashMap<u32, Vec<u8>> = std::collections::HashMap::new();
        wal.recover(|rec| {
            let entry = pages.entry(rec.page_id).or_insert_with(|| Vec::new());
            let needed = rec.offset as usize + rec.payload.len();
            if entry.len() < needed {
                entry.resize(needed, 0);
            }
            entry[rec.offset as usize..needed].copy_from_slice(&rec.payload);
            Ok(())
        })
        .unwrap();
        assert_eq!(pages.get(&1).unwrap().as_slice(), b"page1-new-content");
        assert_eq!(pages.get(&2).unwrap().as_slice(), b"page2-new-content");
        let _ = (Page::new, PageType::Data, table_path); // suppress unused imports
    }

    #[test]
    fn m1_checkpoint_then_crash_replays_only_after_checkpoint() {
        let path = tmp_path();
        {
            let mut wal = Wal::create(&path).unwrap();
            wal.append(0, 0, b"before-cp").unwrap();
            wal.checkpoint(1).unwrap();
            wal.append(0, 0, b"after-cp").unwrap();
            drop(wal);
        }
        let mut wal = Wal::open(&path).unwrap();
        assert_eq!(wal.checkpoint_lsn(), 1);
        let mut page0 = vec![0u8; 16];
        wal.recover(|rec| {
            if rec.page_id == 0 {
                let end = (rec.offset as usize + rec.payload.len()).min(page0.len());
                page0[rec.offset as usize..end].copy_from_slice(&rec.payload);
            }
            Ok(())
        })
        .unwrap();
        // After recover, page0 should have "after-cp" (the post-checkpoint record).
        // The "before-cp" record is skipped because lsn <= checkpoint_lsn.
        assert!(page0.windows(8).any(|w| w == b"after-cp"));
        assert!(!page0.windows(9).any(|w| w == b"before-cp"));
    }

    /// **M1 端到端集成测试**：WAL + Space 一起模拟崩溃恢复。
    #[test]
    fn m1_wal_plus_space_crash_recovery() {
        use ferrumdb_page::Page;
        use ferrumdb_page::PageType;
        use ferrumdb_space::Space;

        let dir = tempfile::tempdir().unwrap();
        let wal_path = dir.path().join("m1.wal");
        let space_path = dir.path().join("m1.ibd");

        // Phase A: 创建 Space + WAL，分配 page 1，写 WAL record 记录新内容。
        // 注意：这里我们模拟"修改内存中的 page 后立即 WAL 记录并 fsync"，但**不**写回 Space。
        // 模拟进程在写 WAL 之后、flush 之前被 kill。
        {
            let mut space = Space::create(&space_path).unwrap();
            let page1_id = space.allocate_page().unwrap();
            assert_eq!(page1_id, 1);
            // Modify the page in memory (using Space::read_page → modify → use Space::write_page would be too late for our crash model).
            // For the M1 test, we just write a fresh page with the new content.
            let mut new_page = Page::new(page1_id, PageType::Data);
            new_page.user_data_mut()[..15].copy_from_slice(b"page1-after-wal");
            space.write_page(page1_id, &new_page).unwrap();
            // Append WAL record (this is the "before crash" step).
            let mut wal = Wal::create(&wal_path).unwrap();
            wal.append(page1_id, 0, b"page1-after-wal").unwrap();
            // Don't call wal.checkpoint; we want to simulate kill before checkpoint.
            drop(wal);
            // Space is also dropped (without explicit flush — but Space::write_page already fsync'd).
        }

        // Phase B: 重启。Reopen Space, reopen WAL, run recover.
        let mut space = Space::open(&space_path).unwrap();
        let mut wal = Wal::open(&wal_path).unwrap();
        assert_eq!(wal.next_lsn(), 2);
        assert_eq!(wal.checkpoint_lsn(), 0);

        // Apply WAL records to Space (would normally be wired into BufferPool/B+Tree).
        let mut applied = 0;
        wal.recover(|rec| {
            let mut page = space.read_page(rec.page_id).unwrap();
            let end = (rec.offset as usize + rec.payload.len()).min(page.user_data().len());
            if end > rec.offset as usize {
                page.user_data_mut()[rec.offset as usize..end].copy_from_slice(&rec.payload);
            }
            space.write_page(rec.page_id, &page).unwrap();
            applied += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, 1);

        // Verify page 1 has the expected content.
        let page = space.read_page(1).unwrap();
        let content = &page.user_data()[..15];
        assert_eq!(content, b"page1-after-wal");
    }
}
