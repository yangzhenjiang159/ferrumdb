# 阶段 7a：DML 补全 — 技术设计

## 1. 架构与边界

```
FerrumEngine.update/delete/drop_table
        │  调用
        ▼
TableCatalog (持有 PersistentBtree 句柄)
        │  read/write via PageSource
        ▼
PersistentBtree.get/insert/delete + 新增 all_node_page_ids
        ▼
Space (PageSource)
```

- 修改 `ferrumdb-btree`：`delete` 修复（漏删）+ 新增「遍历所有节点页 id」（供 drop_table 释放页）。
  btree 仍只依赖 `ferrumdb-page`（`PageSource` trait 来自 space，仅以 `&mut S` 泛型出现，非编译期依赖——现状如此，保持）。
- 修改 `ferrumdb-engine`：`engine.rs` 新增 `EngineError::RowNotFound`；`ferrum_engine.rs` 实现三个方法。

## 2. btree delete 修复（R5）

现状：`delete`（persistent.rs:309）下探到叶子后，若 key 不在该叶子立即返回 `Ok(false)`。
但 `get`（:95）下探到叶子后会**沿 `next_leaf` 链表继续找**——注释明确 "leaves may not be
in the same leaf as the separator-key match"。delete 缺这段遍历，可能漏删。

修复：`delete` 在叶子处镜像 `get` 的查找逻辑：

```rust
// 下探到最左可能叶子后：
loop {
    let node = leaf;
    let idx = lower_bound_keys(&keys, key);
    if idx < keys.len() && keys[idx] == key { 移除该条目; write_page; len -= 1; return Ok(true) }
    if idx >= keys.len() {
        // 当前叶子没有更大的 key → 沿链表继续（若有 next_leaf）
        cur = next_leaf;  // None 则返回 false
        continue;
    }
    // key < keys[idx] → key 不在树中
    return Ok(false);
}
```

**不做下溢 rebalance/merge**（v2）。删除可能使叶子键数 < MIN_KEYS，但不破坏正确性
（只影响空间利用），后续阶段再补。

## 3. engine：EngineError::RowNotFound（R6）

`engine.rs` 新增变体：

```rust
#[error("row not found: {0}")]
RowNotFound(String),
```

- `update` 对存在表但不存在行 → `RowNotFound(pk)`。
- `delete` 幂等：不存在行 → `Ok(())`（不报错，MySQL DELETE 0 rows 语义）。
- `drop_table` 不存在表 → `TableNotFound`（已有变体）。
- `StubEngine` 不涉及新变体（update/delete 仍返回 Unsupported）。

## 4. engine：update（R1/R2）

```
update(table, pk, row):
  1. 校验 row.values.len() == schema.columns.len()，否则 Internal
  2. 校验 row.pk 列 == pk（主键不可变，KD2），否则 Internal
  3. 聚簇 get(pk)：None → RowNotFound(pk)
  4. 读旧行（回表无需，聚簇 value 就是整行）
  5. 对每个二级索引：
       old_idx_val = 旧行索引列值；new_idx_val = 新行索引列值
       若 old_idx_val != new_idx_val：
           delete(旧二级 key = encode_secondary_key(old_idx_val, pk))   // 删旧
           insert(新二级 key = encode_secondary_key(new_idx_val, pk))   // 插新
       若相等：不动（二级 value 只依赖 pk，索引键未变无需更新）
  6. 聚簇 insert(pk, encode_row(new_row))   // 覆盖写聚簇 value
```

- 顺序：先改二级（删旧插新）再写聚簇，或反之均可——本阶段单线程无并发可见性要求；
  失败即整体报错，无部分写入保证（同 insert 的先探测后写约定）。
- 唯一索引：update 不改索引列时无需探测；改索引列时新索引键可能撞唯一约束 →
  **插新前探测**（`unique_index_conflict`），冲突返回 `DuplicateKey` 且不做任何写。

## 5. engine：delete（R3）

```
delete(table, pk):
  1. 聚簇 get(pk)：None → Ok(())   // 幂等
  2. 读旧行 → 每个二级索引：delete(encode_secondary_key(index_cols(old_row), pk))
  3. 聚簇 delete(pk)
```

## 6. engine：drop_table（R4）

```
drop_table(name):
  1. catalog.get(name)？None → TableNotFound
  2. 取表全部节点页 id：
       clustered.all_node_page_ids(source)  +  每个二级 all_node_page_ids(source)
  3. 对每个页 id：Space::free_page(id)（归还空闲链表）
  4. catalog 移除表条目
```

**btree 新增 API**（persistent.rs）：

```rust
/// 遍历返回树中所有节点页 id（含 root、内部节点、叶子），供 drop_table 释放页。
pub fn all_node_page_ids<S: PageSource + ?Sized>(&self, source: &mut S) -> Result<Vec<u32>, BTreeError>
```

实现：BFS/DFS 从 root 下探，每访问一个内部节点记录其所有 children 页 id 并继续；
叶子通过 `next_leaf` 链收集。返回去重后的 id 集合。

- free_page 会写 superblock（改 free_list_head）+ 写该页为 Free，多次 free 同一 id 会破坏
  链表 → 必须**先去重**，且按从叶子到根的任意顺序均可（free 不依赖顺序）。
- 页 0（superblock）不会被返回（树 root 不会是 0）。

## 7. 兼容性与契约

- `EngineError` 新增 `RowNotFound` 变体：`ferrumdb-sql` object-safe 测试不受影响（不匹配变体）。
- btree `delete` 返回值语义不变（`Ok(true)` 删了 / `Ok(false)` 没有）。
- 无磁盘格式变化；无 superblock 变化（持久化归 7b）。
- `#![deny(missing_docs)]`：engine 与 btree 均已开启，新 pub 项需 doc。

## 8. 风险与回滚

| 风险 | 应对 |
|------|------|
| delete 漏删（叶子链遍历缺失） | 修复后加高分裂回归测试（AC5） |
| drop_table 重复释放页破坏 free list | 先去重；测试验证重新 allocate 复用同一页 id（AC4） |
| update 唯一索引撞键产生部分写入 | 插新前统一探测（复用 insert 路径） |
| RowNotFound 变体遗漏匹配 | grep 全 workspace 匹配 EngineError 处 |

回滚：btree 改动可 revert；engine 三个方法删除即回退到 Unsupported。
