# Database Guidelines — `ferrumdb-server`

## Connection Lifecycle

```
LISTEN  -> ACCEPT  -> HANDSHAKE  -> COMMAND LOOP  -> CLOSE
                                                  -> (on shutdown) DRAIN -> CLOSE
```

Each connection runs through these states in `Session`:

1. **HANDSHAKE**: send `HandshakeV10` packet, read `HandshakeResponse`, validate capabilities
2. **COMMAND LOOP**: read `COM_QUERY` → parse SQL via `ferrumdb-sql` → execute via `Executor` → encode response
3. **CLOSE**: send `OK` or `Error` packet for the last command, drop the connection

## Per-Connection Resources

```rust
pub struct Session {
    peer: SocketAddr,
    stream: TcpStream,
    engine: Arc<dyn StorageEngine>,    // shared, read-only access for SELECT
    sql: Parser,                       // per-connection parser state
}
```

- `Arc<dyn StorageEngine>` allows concurrent SELECTs
- Writes (`INSERT`, `UPDATE`, `DELETE`, `CREATE`) need exclusive access — `tokio::sync::Mutex<Box<dyn StorageEngine>>` is acceptable in v1; later split read/write into two engine types
- Per-connection buffers are bounded; refuse new commands when buffer exceeds a threshold (backpressure)

## Bind Configuration

- Default bind: `0.0.0.0:3306` (MySQL default port)
- Override via env: `FERRUMDB_BIND=127.0.0.1:3333` or CLI flag `--bind 127.0.0.1:3333`
- Read env in `config.rs`, never inline in `main.rs`

## Graceful Shutdown

- Listen for `SIGINT` / `SIGTERM` via `tokio::signal`
- On signal: stop accepting new connections, finish in-flight queries with a deadline, then close
- Log a single `info!` on shutdown start; log per-session close as `debug!`

## Anti-Patterns

- ❌ Hard-coding `127.0.0.1:3306` in `main.rs`
- ❌ Spawning unbounded tasks per connection (DoS risk)
- ❌ Blocking the runtime with a synchronous storage call — the storage stack is sync; wrap with `tokio::task::spawn_blocking`
- ❌ Logging per packet at `info!` level (use `trace!`)
- ❌ Reusing `&mut dyn StorageEngine` across tasks without a mutex
