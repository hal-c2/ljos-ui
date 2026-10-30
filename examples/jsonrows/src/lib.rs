//! All of this example is `Main.qml` and `rows.json`:
//!
//! ```text
//! ljos run examples/jsonrows/Main.qml --watch
//! ```
//!
//! Save `rows.json` while it runs. Rows that did not change keep their
//! delegates, and JSON that cannot be used shows its error over the rows
//! it leaves in place. This crate is only here so the test can hold the
//! example to that.
