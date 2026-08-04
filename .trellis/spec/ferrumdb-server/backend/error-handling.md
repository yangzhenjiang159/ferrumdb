# Error Handling — `ferrumdb-server`

This crate uses **two** error strategies:

1. **`anyhow::Result`** in `main()` for top-level error reporting
2. **Structured errors** from the library crates (`ProtocolError`, `SqlError`, `EngineError`) inside `Session` / `Server`

## Main-Level (`main.rs`)

```rust
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let config = Config::from_env()?;
    let server = Server::bind(config.bind_addr)?;
    server.run().await
}
```

`anyhow` is fine here because `main` has no callers to pattern-match on.

## Per-Session

```rust
async fn handle_command(&mut self, cmd: Command) -> Result<(), ServerError> {
    let stmt = self.sql.parse(cmd.query_text)?;       // SqlError → ServerError via From
    let rows = Executor { engine: &mut *self.engine }.execute(stmt)?;
    self.protocol.encode_rows(rows).await?;            // ProtocolError → ServerError via From
    Ok(())
}
```

`ServerError` (planned) wraps the library errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("protocol: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("sql: {0}")]
    Sql(#[from] SqlError),

    #[error("engine: {0}")]
    Engine(#[from] EngineError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

## Mapping to MySQL Error Packet

| ServerError variant | MySQL error code |
|---------------------|------------------|
| `Protocol(_)` | 1156 / 1159 (capability / truncated) |
| `Sql(SqlError::Parse { .. })` | 1064 (ER_PARSE_ERROR) |
| `Sql(SqlError::TypeError { .. })` | 1366 (ER_TRUNCATED_WRONG_VALUE) |
| `Sql(SqlError::UnsupportedStatement(..))` | 1235 (ER_NOT_SUPPORTED_YET) |
| `Sql(SqlError::Engine(EngineError::TableNotFound(_)))` | 1146 (ER_NO_SUCH_TABLE) |
| `Sql(SqlError::Engine(EngineError::DuplicateKey))` | 1062 (ER_DUP_ENTRY) |
| `Io(_)` | 1158 (ER_NET_PACKET_TOO_LARGE) — fallback |

## Critical Safety Rules

- **Never** use `unwrap()` on a network or storage call
- **Never** leak an `anyhow::Error` into the response — always map to a structured error first
- **Never** log the full query text at `info!` (may contain PII or secrets); use `debug!`

## Anti-Patterns

- ❌ `anyhow` deep inside `Session` — use the structured `ServerError`
- ❌ `panic!` on connection errors — close the connection, log at `warn!`, continue
- ❌ Logging query text at `info!`
