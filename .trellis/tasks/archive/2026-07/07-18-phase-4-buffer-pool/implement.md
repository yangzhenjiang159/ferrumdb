# Implement — 阶段 4 Buffer Pool

## 顺序

```
Step 1:  ferrumdb-buffer 依赖（修改 Cargo.toml）
  └─ Step 2: error.rs + frame.rs + lru.rs
       └─ Step 3: pool.rs（BufferPool 主结构 + fetch/allocate/flush/evict）
            └─ Step 4: guard.rs（PageGuard RAII）
                 └─ Step 5: source.rs（BufferPoolSource 适配器）
                      └─ Step 6: lib.rs 重导出 + 测试
                           └─ Step 7: 集成测试（PersistentBtree 通过 BufferPoolSource）
                                └─ Step 8: cargo test/clippy 全干净
                                     └─ Step 9: spec 同步 + 归档
```

## Step 1 — Cargo.toml

- [ ] 1.1 `crates/ferrumdb-buffer/Cargo.toml` 依赖：`ferrumdb-page`, `ferrumdb-space`, `thiserror`
- [ ] 1.2 dev-dependencies：`tempfile = { workspace = true }`

## Step 2 — error.rs + frame.rs + lru.rs

- [ ] 2.1 `error.rs`: BufferError 5 变体（Io, Page, Space, PoolFull, FrameNotFound）
- [ ] 2.2 `frame.rs`: Frame struct + FrameId newtype
- [ ] 2.3 `lru.rs`: LruVec 简单实现（Vec<FrameId> + touch + pop_lru）

## Step 3 — pool.rs

- [ ] 3.1 BufferPool struct
- [ ] 3.2 BufferPool::with_source(Box<dyn PageSource>, capacity)
- [ ] 3.3 BufferPool::open(path, capacity) + create(path, capacity)
- [ ] 3.4 fetch_page（命中 / 未命中 + 分配 frame / 淘汰 + load）
- [ ] 3.5 allocate_page（调 source.allocate_page + 占 frame）
- [ ] 3.6 unpin(frame_id)（私有，供 guard drop 调用）
- [ ] 3.7 evict_lru（找最久未用未 pinned 未 dirty；如都 dirty 先 flush）
- [ ] 3.8 flush_all + flush_frame
- [ ] 3.9 accessors: capacity, used_frames, dirty_frame_count

## Step 4 — guard.rs

- [ ] 4.1 PageGuard<'a> struct（pool: &'a mut BufferPool, frame_id: FrameId）
- [ ] 4.2 methods: id, page, page_mut, mark_dirty
- [ ] 4.3 Deref<Target=Page> + DerefMut
- [ ] 4.4 Drop 实现（pin_count -= 1 with saturating_sub）

## Step 5 — source.rs

- [ ] 5.1 BufferPoolSource<'a> 持有 `&'a mut BufferPool`
- [ ] 5.2 impl PageSource for BufferPoolSource<'a>
  - read_page: fetch_page + clone Page
  - write_page: fetch_page + page_mut + clone Page in
  - allocate_page: pool.allocate_page + guard.id()

## Step 6 — lib.rs

- [ ] 6.1 重导出 BufferPool, PageGuard, Frame, FrameId, BufferError, BufferPoolSource
- [ ] 6.2 #![deny(missing_docs)]
- [ ] 6.3 单元测试（至少 6 个）：
  - basic fetch + read
  - pin 防止淘汰
  - LRU 淘汰冷页（mock 计数）
  - dirty 页 flush 后淘汰
  - PageGuard drop unpins
  - pool full 返回 PoolFull

## Step 7 — 集成测试

- [ ] 7.1 在 buffer 的 tests 模块加一个集成测试
- [ ] 7.2 1000 keys via BufferPoolSource → flush_all → drop → reopen → verify

## Step 8 — Validation

- [ ] 8.1 `cargo build --workspace` 无 warning
- [ ] 8.2 `cargo test --workspace` 全过；阶段 1+2+3 的 63 个测试不退化
- [ ] 8.3 `cargo clippy --workspace` 0 warning

## Step 9 — Spec 同步 + 归档

- [ ] 9.1 更新 `.trellis/spec/ferrumdb-buffer/backend/` 6 个文件
- [ ] 9.2 写 `research/lessons.md`
- [ ] 9.3 prd.md 12 项全勾选
- [ ] 9.4 `task.py finish` + `task.py archive`

## Review gates

每步完成后：
- [ ] `cargo test -p ferrumdb-buffer` 通过
- [ ] 没有引入 `unsafe` / `unwrap()` / 新外部依赖
- [ ] `#![deny(missing_docs)]` 不破
- [ ] 新增 pub 项有 `///` doc

## Rollback

BufferPool 通过 PageSource trait 与 PersistentBtree 解耦。如果 BufferPool 有严重 bug，可以删 ferrumdb-buffer 而不影响持久化 B+Tree 的运行。
