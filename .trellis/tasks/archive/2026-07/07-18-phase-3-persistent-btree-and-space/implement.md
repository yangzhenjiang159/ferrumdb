# Implement — 阶段 3 持久化 B+Tree + 表空间

## 顺序

```
Step 1:  Workspace 依赖（tempfile）
  └─ Step 2: ferrumdb-space error.rs + superblock.rs
       └─ Step 3: ferrumdb-space free_list.rs
            └─ Step 4: ferrumdb-space space.rs（Space + open/create/read/write/alloc/free）
                 └─ Step 5: ferrumdb-space page_source.rs
                      └─ Step 6: ferrumdb-btree persist.rs（节点 ↔ Page 编解码）
                           └─ Step 7: ferrumdb-btree PersistentBtree + PageSource impl
                                └─ Step 8: 集成测试（create → insert → reopen → verify）
                                     └─ Step 9: 全 cargo test + clippy
                                          └─ Step 10: spec 同步 + 归档
```

## Step 1 — Workspace deps

- [ ] 1.1 根 `Cargo.toml` 加 `tempfile = "3"` 到 `[workspace.dependencies]`
- [ ] 1.2 `crates/ferrumdb-space/Cargo.toml` 加 `tempfile = { workspace = true }` 到 `[dev-dependencies]`
- [ ] 1.3 `crates/ferrumdb-btree/Cargo.toml` 加 `tempfile = { workspace = true }` 到 `[dev-dependencies]`

## Step 2 — space/error.rs + space/superblock.rs

- [ ] 2.1 `error.rs`: `SpaceError` 8 变体（Io, PageIdOutOfRange, FreeListCorrupted, SuperblockInvalidMagic, SuperblockPageSizeMismatch, SuperblockVersionUnsupported, NotInitialized, ...预留）
- [ ] 2.2 `superblock.rs`: `Superblock` struct + `to_bytes(&self) -> Vec<u8>` + `from_bytes(&[u8]) -> Result<Self, SpaceError>`
- [ ] 2.3 superblock 编码固定 26 字节（前 26 字节有意义，剩余 0）

## Step 3 — space/free_list.rs

- [ ] 3.1 内部 helper：`encode_free_page(next: Option<PageId>) -> [u8; 5]` + `decode_free_page(&[u8]) -> Result<Option<PageId>, SpaceError>`
- [ ] 3.2 仅由 space.rs 使用，不 pub

## Step 4 — space/space.rs

- [ ] 4.1 `Space` struct + `path`, `file`, `superblock`, `dirty` 字段
- [ ] 4.2 `Space::create(path)` 流程：create + truncate + write superblock page 0 + set_len to 1 page + sync_all
- [ ] 4.3 `Space::open(path)` 流程：open + read page 0 + parse superblock + validate magic/version/page_size
- [ ] 4.4 `read_page(id)`：seek + read_exact + Page::from_bytes
- [ ] 4.5 `write_page(id, page)`：seek + write_all + sync_all
- [ ] 4.6 `allocate_page()`：pop free list head → extend file → set PageType::Free + return
- [ ] 4.7 `free_page(id)`：读页 → 写 free list 头到 user_data → 改 page_type 为 Free → write_page → update superblock → sync_all
- [ ] 4.8 `set_root_page_id(id)` + `sync_all()`

## Step 5 — space/page_source.rs

- [ ] 5.1 `PageSource` trait：`read_page`, `write_page`, `allocate_page`
- [ ] 5.2 `impl PageSource for Space`（直接 forward）

## Step 6 — btree/persist.rs

- [ ] 6.1 节点 user_data 序列化（kind, key_count, is_root, keys, children/values, next_leaf）
- [ ] 6.2 `encode_node(node: &NodeRef, page: &mut Page) -> Result<(), BTreeError>`
- [ ] 6.3 `decode_node(page: &Page) -> Result<NodeRef, BTreeError>`
- [ ] 6.4 `NodeRef` enum：`Internal { keys, children }` / `Leaf { keys, values, next }`（不带 Box，用 PageId 表示子节点）

## Step 7 — btree/tree.rs PersistentBtree

- [ ] 7.1 `PersistentBtree<K, V>` struct
- [ ] 7.2 `PersistentBtree::create(space: &mut Space) -> Result<Self, BTreeError>`：alloc root page + 写 empty leaf
- [ ] 7.3 `PersistentBtree::open(space: &mut Space, root: PageId) -> Result<Self, BTreeError>`：读 root page + 计算 height
- [ ] 7.4 `insert(key, value)`：递归（不预读 root 入栈，每次都从 source 读）
- [ ] 7.5 根分裂处理 + set_root_page_id
- [ ] 7.6 `get(key)` + `scan_range(start, end) -> Vec<(K, V)>`（v1 简化为收集 Vec）

## Step 8 — 集成测试

- [ ] 8.1 `crates/ferrumdb-space/tests/integration.rs` 或在 `space.rs` 的 tests 模块
- [ ] 8.2 `crates/ferrumdb-btree/tests/integration.rs` 或在 `tree.rs` 的 tests 模块
- [ ] 8.3 至少 5 个集成测试：create-open, bad-magic, alloc-free, 1000-keys-reopen, scan-range-persisted

## Step 9 — 验证

- [ ] 9.1 `cargo build` 无 warning
- [ ] 9.2 `cargo test` 全过；阶段 1+2 的 34 个测试不退化
- [ ] 9.3 `cargo clippy --workspace` 0 warning

## Step 10 — Spec 同步 + 归档

- [ ] 10.1 更新 `ferrumdb-space` 4 个 spec 文件
- [ ] 10.2 更新 `ferrumdb-btree` 4 个 spec 文件（新增 PersistentBtree 部分）
- [ ] 10.3 写 `research/lessons.md`
- [ ] 10.4 prd.md 11 项全勾选
- [ ] 10.5 `task.py finish` + `task.py archive`

## Review gates

每步完成后：
- [ ] `cargo test -p <crate>` 通过
- [ ] 没有引入 `unsafe` / `unwrap()` / 新外部依赖（除 tempfile）
- [ ] `#![deny(missing_docs)]` 不破
- [ ] 新增 pub 项有 `///` doc
- [ ] 错误变体新增时同步更新对应 error-handling.md

## Rollback

任一步出错：
- 该步骤文件改动通过 `git restore <path>` 回滚
- 上一阶段测试应继续通过
- 节点序列化格式若需修改，bump `Superblock::version` 字段
