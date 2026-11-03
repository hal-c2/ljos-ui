//! All of this example is `Main.qml` and the pages in `pages/`:
//!
//! ```text
//! cargo build -p ljos_cli --features webengine
//! cargo build -p ljos_webengine --features cef --bin ljos-webengine-helper
//! target/debug/ljos run examples/webbrowser/Main.qml
//! ```
//!
//! It shows a browser: back and forward through the pages it has shown,
//! reload or stop, a URL field, the page's title as `document.title`
//! reads it, and what went wrong when a page cannot be shown (the home
//! page links to one that is not there). A build without the web engine
//! shows that every page fails, and says why. This crate is only here so
//! the test can hold the example to that.
