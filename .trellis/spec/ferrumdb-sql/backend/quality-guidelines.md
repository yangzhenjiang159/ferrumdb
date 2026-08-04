# Quality Guidelines — `ferrumdb-sql`

## Required Patterns

1. **`#![deny(missing_docs)]`** — set in `crates/ferrumdb-sql/src/lib.rs:13`.

2. **Module `//!` doc** in standard format (Chinese responsibilities + phase 8 reference).

3. **Error type in `error.rs`**, `thiserror` derive, with `Span` for parser errors.

4. **Parser and executor are separate**: `parse(sql) -> Result<Statement, SqlError>` then `Executor::execute(stmt) -> Result<Rows, SqlError>`.

5. **Executor borrows `&mut dyn StorageEngine`** — does not own it.

6. **Object-safety assertion preserved** at `crates/ferrumdb-sql/src/lib.rs:14-16`.

## Forbidden Patterns

| Anti-pattern | Why |
|--------------|-----|
| Owning the engine | Server needs to share one engine across sessions |
| Pulling in `sqlparser` without design review | Hand-written is small enough |
| `JOIN` / subquery support in v1 | Out of scope |
| Returning `String` for errors | Use the enum variants with `Span` |
| Logging inside the executor | Caller logs with request context |
| Executing in the parser | Mixing concerns |

## Testing Requirements

When implementation lands:

- [ ] `CREATE TABLE` parser: positive + column-count-mismatch negative
- [ ] `INSERT` parser: positive + arity-mismatch negative
- [ ] `SELECT` parser: positive + unknown-column negative
- [ ] `Executor::execute` round-trip through a `StorageEngine` mock for each statement kind
- [ ] `SqlError::Parse` carries correct `Span`
- [ ] `EngineError` from the mock propagates as `SqlError::Engine`
- [ ] Object-safety test continues to pass

## Code Review Checklist

- [ ] Module `//!` doc references phase 8
- [ ] No new external dep
- [ ] All `pub` items have `///` doc comments
- [ ] Parser and executor are in separate files
- [ ] `Executor` borrows, never owns
- [ ] Object-safety assertion still compiles
- [ ] `cargo test -p ferrumdb-sql` passes
