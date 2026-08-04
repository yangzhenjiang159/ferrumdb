# Error Handling — `ferrumdb-sql`

## Planned Error Type

```rust
// crates/ferrumdb-sql/src/error.rs (planned)
#[derive(Debug, thiserror::Error)]
pub enum SqlError {
    /// Parser error at a known span.
    #[error("parse error at {span}: {message}")]
    Parse { span: Span, message: String },

    /// Unknown statement type (e.g. `DROP` in v1).
    #[error("unsupported statement at {span}: {0}")]
    UnsupportedStatement(Span, String),

    /// Type error (e.g. `INSERT` with wrong arity).
    #[error("type error at {span}: {0}")]
    TypeError(Span, String),

    /// Wrapped `EngineError` from the storage engine.
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),
}
```

## Conventions

- `thiserror::Error` derive
- `#[from] EngineError` so `?` works in the executor
- `Span` carries `line` + `column` (1-based) for every parser/type error
- **No** `Internal(String)` — every error has a category

## Propagation Pattern

```rust
fn execute(&mut self, stmt: Statement) -> Result<Rows, SqlError> {
    match stmt {
        Statement::CreateTable { name, schema } => {
            self.engine.create_table(&name, schema)?;  // EngineError → SqlError via From
            Ok(Rows::Empty)
        }
        // ...
    }
}
```

## Critical Safety Rules

- **Never** swallow an `EngineError` — let it propagate with `?`
- **Never** fabricate a fake `Span` for an error that came from the engine (it doesn't have one)
- **Never** log inside the executor

## Anti-Patterns

- ❌ Returning `String` errors — use the enum variants
- ❌ Hiding parse-error position — `Span` is required
- ❌ `panic!` on unsupported statements — return `UnsupportedStatement`
