//! A build without the `woff` feature refuses a packed font by its format,
//! so the error names what is missing rather than calling the file corrupt.
#![cfg(not(feature = "woff"))]

use ljos_text::{FontError, TextEngine};

#[test]
fn a_woff2_or_woff_font_is_a_format_this_build_does_not_read() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../features/text/fonts/");
    let mut e = TextEngine::new();
    for (name, format) in [
        ("LjosSymbolsTest-Regular.woff2", "WOFF2"),
        ("LjosSymbolsTest-Regular.woff", "WOFF"),
    ] {
        let bytes = std::fs::read(format!("{dir}{name}")).unwrap();
        let err = e.register_font(bytes).unwrap_err();
        assert!(
            matches!(err, FontError::Unsupported(f) if f == format),
            "{name}: {err:?}"
        );
        assert_eq!(
            err.to_string(),
            format!("this build reads no {format} fonts (the `woff` feature is off)")
        );
    }
}
