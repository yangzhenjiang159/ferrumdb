# 协作与 Code Review 约定

## 分工

| 角色 | 职责 |
|------|------|
| **开发者（你）** | 按 [plan.md](plan.md) 手写实现与测试 |
| **助手** | 提供设计说明、回答疑问、Review 代码并给出改进建议 |

助手**默认不直接改业务代码**；若需要代为修改，需明确说明。

---

## 开发流程

1. 阅读当前阶段在 [plan.md](plan.md) 中的步骤与验收标准
2. 在对应 crate 中实现
3. 自测：`cargo test`（必要时 `cargo clippy`）
4. 发起 Review：提供目标、文件路径、疑问、自测结果
5. 根据 Review 修改，进入下一阶段

---

## 提交 Review 时请包含

```markdown
## 阶段
例如：阶段 1 — Page 基础

## 目标
完成了什么

## 相关文件
- crates/ferrumdb-page/src/...

## 自测
cargo test -p ferrumdb-page
结果：全部通过

## 疑问（可选）
例如：checksum 用 CRC32 还是 xxHash？
```

---

## Review 维度

1. **正确性**：是否满足验收标准；边界与错误路径
2. **设计**：模块边界、与 architecture 是否一致；是否为下一阶段留好接口
3. **Rust 惯用法**：所有权、错误类型、`unsafe` 必要性
4. **InnoDB 语义**：页、WAL、B+Tree 行为是否合理
5. **测试**：是否覆盖 round-trip、崩溃/损坏等关键场景
6. **文档**：公开 API 的 doc comment

---

## 改进建议格式

Review 反馈通常分类为：

- **必须改**：正确性 bug、数据丢失风险、违反架构依赖
- **建议改**：可读性、性能、更符合 Rust 习惯
- **可选**：命名、注释、后续 refactor

---

## 当前任务

👉 **[阶段 2 — 行编码 + 内存 B+Tree](plan.md#阶段-2--行编码--内存-btree)**
