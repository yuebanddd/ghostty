# GTTY custom layer

This directory owns GTTY-specific code. It is intentionally separate from
Ghostty's terminal, renderer, PTY, and platform application code.

The initial workspace contains:

- `crates/gtty-protocol` — versioned IPC envelopes, negotiation, and framing.
- `crates/gttyd` — the local daemon skeleton and Unix socket transport.
- `protocol/v0/examples` — executable JSON examples for IPC v0.

Run the Rust checks from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
