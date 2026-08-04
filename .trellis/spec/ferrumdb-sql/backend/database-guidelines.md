# Database Guidelines — `ferrumdb-sql`

## Supported Grammar (v1)

```sql
CREATE TABLE t (
    id INT PRIMARY KEY,
    name VARCHAR(255)
);

INSERT INTO t VALUES (1, 'hello');

SELECT * FROM t WHERE id = 1;
SELECT * FROM t WHERE id BETWEEN 1 AND 10;
```

Out of scope for v1: `JOIN`, subqueries, `UPDATE`, `DELETE`, transactions,
prepared statements, `ALTER TABLE`.

## AST Sketch

```rust
pub enum Statement {
    CreateTable { name: String, schema: Schema },
    Insert { table: String, values: Vec<Value> },
    Select { table: String, where_: Option<Expr> },
}

pub enum Expr {
    Eq(Value, Value),
    Between(Value, Value, Value),
    // ... only what's needed for v1
}
```

## Parser Style

Hand-written recursive descent:

```rust
impl Parser {
    fn parse_statement(&mut self) -> Result<Statement, SqlError> { ... }
    fn parse_create_table(&mut self) -> Result<Statement, SqlError> { ... }
    fn parse_insert(&mut self) -> Result<Statement, SqlError> { ... }
    fn parse_select(&mut self) -> Result<Statement, SqlError> { ... }
}
```

Each rule returns `Result<Statement, SqlError>` with a `Span` indicating the
error position.

## Executor

```rust
pub struct Executor<'a> {
    engine: &'a mut dyn StorageEngine,
}

impl<'a> Executor<'a> {
    pub fn execute(&mut self, stmt: Statement) -> Result<Rows, SqlError> {
        match stmt {
            Statement::CreateTable { name, schema } => {
                self.engine.create_table(&name, schema)?;
                Ok(Rows::Empty)
            }
            Statement::Insert { table, values } => {
                let row = Row { values };
                self.engine.insert(&table, row)?;
                Ok(Rows::Empty)
            }
            Statement::Select { table, where_ } => {
                let range = where_.map(|w| w.to_range()).unwrap_or_else(RangeBound::full);
                let iter = self.engine.scan(&table, range)?;
                Ok(Rows::Iter(iter))
            }
        }
    }
}
```

## `Rows` Return Type

```rust
pub enum Rows<'a> {
    Empty,
    Iter(RowIterator<'a>),
}
```

`SELECT` returns `Iter`; `CREATE` / `INSERT` return `Empty`. The caller
(`ferrumdb-server`) decides how to serialise.

## Anti-Patterns

- ❌ Pulling in `sqlparser` "just because" — hand-written is small and clear for this grammar
- ❌ Executing in the parser — separate parsing from execution
- ❌ Owning the engine (`Executor<'a>` borrows from `&mut dyn StorageEngine`)
- ❌ Returning `String` error messages without a `Span` — callers need position info
- ❌ Adding `JOIN` / subquery support in v1 (deferred)
