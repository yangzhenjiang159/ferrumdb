# Journal - yangzhenjiang (Part 1)

> AI development session journal
> Started: 2026-08-05

---



## Session 1: 阶段6：二级索引与回表

**Date**: 2026-08-06
**Task**: 阶段6：二级索引与回表
**Branch**: `master`

### Summary

实现非主键二级索引最小引擎：IndexMeta、每表聚簇+多二级 B+Tree、insert 同步维护、get_by_index 回表、scan_index 范围扫描、唯一索引冲突检查；110 测试全绿、clippy -D warnings 干净。

### Main Changes

- 新增 ferrumdb-page/src/key.rs 保序键编码（类型标签前缀无关：Null/I64/Bytes + 0x00-escape + successor 前缀扫描上界）
- StorageEngine trait 新增 IndexMeta + create_index/get_by_index/scan_index（object-safe），StubEngine 同步
- 新增 FerrumEngine 最小引擎（内存 catalog + Space 持久树），实现 create_table/insert/get_by_pk/get_by_index/scan/scan_index
- 集成测试 AC1-AC7 全覆盖（点查/范围回表/多索引一致/持久化重开/唯一冲突无部分写入/非唯一共享 key）

### Git Commits

| Hash | Message |
|------|---------|
| `4d9bb7d` | (see git log) |
| `7e32126` | (see git log) |
| `c740465` | (see git log) |
| `c5da02a` | (see git log) |

### Testing

- [OK] cargo build 0 error；110 测试全绿（基线 85）；clippy --all-targets -D warnings 干净

### Status

[OK] **Completed**

### Next Steps

- 阶段7：StorageEngine 整合（catalog 持久化、update/delete 及索引维护、BufferPool 接入）


## Session 2: 阶段7a：DML 补全（update/delete/drop_table）

**Date**: 2026-08-06
**Task**: 阶段7a：DML 补全（update/delete/drop_table）
**Branch**: `master`

### Summary

实现 StorageEngine 剩余写路径：update（主键不可变、索引列变则删旧插新、唯一先探测）、delete（幂等）、drop_table（释放全部树页）；修复 btree delete 漏删（叶子链遍历），新增 all_node_page_ids；新增 RowNotFound 变体。122 测试全绿、clippy 干净。

### Main Changes

- 修复 PersistentBtree::delete 漏删：镜像 get 的 next_leaf 叶子链遍历，高分裂场景不漏删
- 新增 PersistentBtree::all_node_page_ids（BFS 去重），供 drop_table 释放页
- engine 实现 update/delete/drop_table；catalog 新增 remove；EngineError 新增 RowNotFound
- 集成测试 AC1-AC5：update 索引一致性/RowNotFound/改pk拒绝/唯一冲突、delete 幂等、drop_table 页复用

### Git Commits

| Hash | Message |
|------|---------|
| `96a0354` | (see git log) |
| `39eba2c` | (see git log) |
| `e62ccf2` | (see git log) |
| `0a255fe` | (see git log) |

### Testing

- [OK] cargo build 0 error；122 测试全绿；clippy -D warnings 干净；sql object-safe 通过

### Status

[OK] **Completed**

### Next Steps

- 阶段7b：catalog 持久化（JSON superblock 扩展区）+ WAL 整页 redo + 崩溃恢复
