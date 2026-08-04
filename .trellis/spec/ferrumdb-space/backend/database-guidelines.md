# Database Guidelines — `ferrumdb-space`

On-disk tablespace file format conventions.

---

## File Layout

```
tablespace.ibd
├── Page 0       Superblock  (PageType::Superblock)
├── Page 1       First allocated page (B+Tree root, free list head, or future catalog)
├── Page 2..N    ...
```

File length = `n * PAGE_SIZE` (always a multiple of 16384). Extended via `std::fs::File::set_len`.

## Superblock (26 bytes in page 0 user_data)

Little-endian, fixed layout:

```
[magic: u32 LE]               offset 0, = PAGE_MAGIC = 0xFEDB_0001
[version: u32 LE]            offset 4, currently 1
[page_size: u32 LE]          offset 8, = 16384
[free_list_head_is_some: u8] offset 12, 0 or 1
[free_list_head: u32 LE]     offset 13
[root_page_id_is_some: u8]    offset 17
[root_page_id: u32 LE]       offset 18
[last_lsn: u64 LE]           offset 22
```

Page 0's 32-byte PageHeader + 8-byte PageFooter + CRC32 still wrap this content, providing a second validation layer.

## Free List

Each free page has `PageType::Free`; its user_data carries:

```
[next_is_some: u8]            offset 0
[next_page_id: u32 LE]        offset 1
```

5 bytes. The list is singly-linked; `Space.superblock.free_list_head` points to the head. Allocation pops from the head; freeing prepends to the head.

## PageId ↔ File Offset

```rust
fn offset_of(page_id: u32) -> u64 {
    page_id as u64 * PAGE_SIZE as u64
}
```

The ONLY place this math happens, in `space.rs`. Every read/write/extend calls it.

## Fsync Discipline

`Space::sync_all()` (forwarding to `File::sync_all`) is called after:

- `Space::create` (initial superblock write)
- `Space::open` (no — read-only validation)
- `Space::write_page` (every write)
- `Space::allocate_page` (extend + new page + superblock re-write)
- `Space::free_page` (free page write + superblock update)
- `Space::set_root_page_id` (superblock re-write)
- `Space::write_superblock` (private helper)

## Page Allocation Algorithm

```
allocate_page():
    if superblock.free_list_head is Some(id):
        page = read_page(id)
        next = decode_free(page.user_data)
        superblock.free_list_head = next
        write_superblock()
        zero_out page user_data
        write_page(id, page)
        return id
    else:
        new_id = page_count
        set_len((page_count + 1) * PAGE_SIZE)
        page_count += 1   # BEFORE write_page
        new_page = Page::new(new_id, PageType::Free)  # zero-initialized
        write_page(new_id, new_page)
        write_superblock()  # record any state change
        return new_id
```

## Anti-Patterns

- ❌ Redefining `PAGE_SIZE` locally
- ❌ Skipping `sync_all` after metadata mutations
- ❌ Auto-truncating on corruption — return error and let operator decide
- ❌ Holding a `Space` borrow across an await point (sync crate)
- ❌ Returning `Ok(())` from a write that errored mid-way
- ❌ Calling `write_page(new_id, ...)` BEFORE incrementing `page_count` (causes PageIdOutOfRange)
