# CLOB simulator

Rust 1.93.0. No external crate dependencies.

Build with `cargo build --locked --offline`.
Run `./target/debug/clob-sim`; it reads a script from standard input and writes
records to standard output. The interface and required behavior are defined in
[SPEC.md](SPEC.md).
