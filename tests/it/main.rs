//! The crate's integration tests in one binary.
//!
//! Every former `tests/<name>.rs` is a module here, so a warm edit loop links one test
//! executable instead of one per file. The test names keep their module prefix
//! (`<name>::<test>`); `tests/internal` stays where the library compiles it (`#[path]`).
//! Both modules are empty without the `conformance` feature (their inner `#![cfg]`).
mod contract;
mod validation;
