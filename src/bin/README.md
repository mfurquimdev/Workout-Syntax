# src/bin

The crate's three command-line programs. Each is a thin wrapper around
the library.

## Files

| File | Binary | Purpose |
|---|---|---|
| `workout-lint.rs` | `workout-lint` | `workout-lint <file> [--sets]`. Runs `parse_document` on a file and prints each diagnostic with its line. `--sets` also prints every session with its expanded sets (`Exercise::expand`, `format_sets`). Exits 1 when there is an error. Needs no feature. |
| `workout-lsp.rs` | `workout-lsp` | The language server over stdio. Opens an `lsp_server` stdio connection and calls `lsp::server::run`. Needs `native`. |
| `workout-server.rs` | `workout-server` | The sync server. Parses `--log`, `--listen` (default `127.0.0.1:8484`), `--app` and `--allow-host`, opens a `sync::LogStore` and calls `server::serve`. `127.0.0.1` and `localhost` are always allowed hosts. Warns when the listen address is not a loopback or VPN address. Needs `native`. |

## How they relate

- `workout-lint` uses `document` and `expand` (see [`../README.md`](../README.md)).
- `workout-lsp` uses [`../lsp/server.rs`](../lsp/README.md), which uses `lsp/analysis.rs`.
- `workout-server` uses `server.rs`, which uses `sync.rs`.

`Cargo.toml` marks `workout-lsp` and `workout-server` with
`required-features = ["native"]`. `cargo install --path .` installs all
three into `~/.cargo/bin`.

Example:

```sh
cargo run --bin workout-server -- --log copy/Workout.md --allow-host workout.example.com
```
