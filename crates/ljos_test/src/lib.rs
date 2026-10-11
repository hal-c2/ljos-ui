//! Test support for Ljós (ARCHITECTURE.md sections 14, 17 and 18.1).
//!
//! - [`a11y`]: the accessibility audit that runs on a mounted scene.
//! - The `QtTest` module (`TestCase`, `SignalSpy`), so that an
//!   application's existing `tst_*.qml` suites keep running. It is an
//!   import path for tests that exist; new behaviour is specified in
//!   `features/`.
//!
//! The crate reads compiled IR like every runtime crate and does not link
//! the parser (CLAUDE.md rule 5): `ljos test` compiles the test files.

pub mod a11y;
