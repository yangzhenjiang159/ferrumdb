# Backend Development Guidelines — `ferrumdb-protocol`

> MySQL Wire Protocol: handshake, authentication, OK/Error/ResultSet packet encoding/decoding.

---

## Overview

`ferrumdb-protocol` owns the wire-format layer that lets a MySQL-compatible
client (e.g. the `mysql` CLI) talk to FerrumDB. It is responsible for the
handshake response, command packet parsing, and the OK / Error / ResultSet
packet encoding.

Today the crate is a stub (`crates/ferrumdb-protocol/src/lib.rs`). Real
implementation lands in phase 8 per `docs/plan.md`.

The crate depends on `bytes` (for zero-copy packet slicing) and `thiserror`.
It does **not** depend on `ferrumdb-engine`, `ferrumdb-sql`, or
`ferrumdb-server` — those wire protocol in later.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Stub + planned files | Filled |
| [Database Guidelines](./database-guidelines.md) | MySQL packet format, charset, capabilities | Filled |
| [Error Handling](./error-handling.md) | Planned `ProtocolError` | Filled |
| [Logging Guidelines](./logging-guidelines.md) | Library, no logging | Filled |
| [Quality Guidelines](./quality-guidelines.md) | Packet round-trip, charset pinning, tests | Filled |

---

## Pre-Development Checklist

- [ ] Pin charset to one value (`docs/plan.md` 阶段 8 常见坑: "字符集先固定 `utf8mb4` 或 `latin1`") — recommend `utf8mb4`
- [ ] No `async` — pure encode/decode is sync
- [ ] Use `bytes::Bytes` for packet payloads (zero-copy slicing)
- [ ] Length-prefixed framing: 3-byte little-endian length + 1-byte sequence id + payload
- [ ] Capabilities bitfield fixed at implementation time (do not negotiate dynamically)
- [ ] No prepared-statement support in v1 (per `docs/plan.md` 阶段 8)

---

## Quality Check (Reviewer Gate)

- [ ] Module `//!` doc references phase 8
- [ ] `ProtocolError` in `error.rs`, `thiserror` derive
- [ ] All `pub` items have `///` doc comments
- [ ] Packet round-trip test: encode → decode → assert equal
- [ ] Charset documented in module doc
- [ ] No `unsafe`
