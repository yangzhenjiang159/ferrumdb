# Directory Structure — `ferrumdb-sql`

Real layout (2026-07-18):

```
crates/ferrumdb-sql/
├── Cargo.toml              # ferrumdb-engine + ferrumdb-page + thiserror
└── src/
    └── lib.rs              # Module doc + #![deny(missing_docs)] + object-safety smoke test
```

The object-safety test at `crates/ferrumdb-sql/src/lib.rs:14-16` is the only
content beyond the module doc today.

## Planned (not yet present)

| File | Purpose | Phase |
|------|---------|-------|
| `error.rs` | `SqlError` (thiserror, includes `Span`) | 8 |
| `lexer.rs` | Tokeniser | 8 |
| `parser.rs` | Recursive-descent parser → `Statement` AST | 8 |
| `ast.rs` | `Statement` enum + sub-types | 8 |
| `executor.rs` | `Executor` that runs `Statement` against `&mut dyn StorageEngine` | 8 |
| `span.rs` | `Span { line, column }` for error reporting | 8 |
