# Backend Development Guidelines — `ferrumdb-sql`

> SQL parser and minimal execution for FerrumDB: CREATE / INSERT / SELECT on a tiny grammar.

---

## Overview

`ferrumdb-sql` parses the minimal SQL subset needed for phase 8 and turns
each statement into a `StorageEngine` call. The parser is hand-written
(recursive descent) unless `sqlparser` is chosen later (the plan flags this
as an evaluation: "手写递归下降 parser（或先用 `sqlparser` crate 评估）").

Today the crate is a stub (`crates/ferrumdb-sql/src/lib.rs`). The only
non-trivial content is a compile-time assertion that `StorageEngine` is
object-safe:

```rust
fn _assert_object_safe(_: &dyn StorageEngine) {}
```

(`crates/ferrumdb-sql/src/lib.rs:14-16`)

Real implementation lands in phase 8 per `docs/plan.md`.

The crate depends on `ferrumdb-engine` (for `StorageEngine`) and
`ferrumdb-page` (for `Value` / `Row` / `Schema`). It does **not** depend on
`ferrumdb-protocol` or `ferrumdb-server` — those wire SQL in later.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Stub + planned files | Filled |
| [Database Guidelines](./database-guidelines.md) | Grammar, AST, executor | Filled |
| [Error Handling](./error-handling.md) | Planned `SqlError` | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Parser hygiene, executor purity, tests | Filled |

---

## Pre-Development Checklist

- [ ] Decide parser strategy: hand-written recursive descent (recommended) vs `sqlparser` crate evaluation
- [ ] Supported grammar in v1: `CREATE TABLE`, `INSERT INTO ... VALUES`, `SELECT ... FROM ... WHERE` (no JOIN, no subqueries)
- [ ] Charset is `utf8mb4` (matches `ferrumdb-protocol`)
- [ ] Executor holds `&mut dyn StorageEngine`, never owns it
- [ ] No `async`
- [ ] Errors include 1-based line + column for parser errors

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references phase 8
- [ ] `SqlError` in `error.rs`, `thiserror` derive, includes span info for parser errors
- [ ] All `pub` items have `///` doc comments
- [ ] Object-safety assertion in `crates/ferrumdb-sql/src/lib.rs:14-16` continues to pass
- [ ] Parser tests: each grammar rule has positive + negative test cases
- [ ] Executor tests: each statement type round-trips through a `StorageEngine` mock
