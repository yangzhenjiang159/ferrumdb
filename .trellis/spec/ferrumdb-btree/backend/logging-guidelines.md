# Logging Guidelines — `ferrumdb-btree`

## Rule: No Logging In This Crate

`ferrumdb-btree` is a library. The current stub
(`crates/ferrumdb-btree/src/lib.rs`) contains zero `tracing` or `log` calls,
and the `Cargo.toml` lists neither as a dependency. This must stay true
through every phase.

### Why

- B+Tree operations are on the hot path; logging per insert/scan would dominate the cost
- This crate has no `tracing` dependency, and adding one would create a circular concern (every consumer of `ferrumdb-btree` would transitively pull `tracing`)
- Errors already carry full context via `BTreeError` variants; the caller decides what to log

### Cross-Reference

- `ferrumdb-page`: same rule (see `.trellis/spec/ferrumdb-page/backend/logging-guidelines.md`)
- `ferrumdb-engine`: when its `insert` / `scan` calls fail, it should wrap the `BTreeError` into `EngineError::Internal(...)` and log at `warn!` or `error!` if the call site warrants it
- `ferrumdb-server`: only crate allowed to log; see its logging guidelines

### When Implementation Lands

If a future phase finds a real need for diagnostics inside the tree (e.g. profiling splits), the rule is:
1. Use `tracing::trace!` (not `info!` / `warn!`) — `trace` is off by default
2. The `Cargo.toml` gains `tracing = { workspace = true }` only after a public design discussion
3. The change is reflected here before the PR merges
