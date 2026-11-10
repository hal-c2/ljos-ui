//! Fonts packed as WOFF2 and WOFF: unpacked before they are registered, and
//! refused with a reason, never a panic, when they are broken.
#![cfg(feature = "woff")]

use ljos_text::{FontError, TextEngine};

fn fixture(name: &str) -> Vec<u8> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../features/text/fonts/");
    std::fs::read(format!("{dir}{name}")).unwrap()
}

#[test]
fn a_woff2_or_woff_font_registers_its_family() {
    for name in [
        "LjosSymbolsTest-Regular.woff2",
        "LjosSymbolsTest-Regular.woff",
    ] {
        let mut e = TextEngine::new();
        assert_eq!(
            e.register_font(fixture(name)).unwrap(),
            ["Ljos Symbols Test"],
            "{name}"
        );
        let faces = TextEngine::new()
            .register_font_faces(fixture(name))
            .unwrap();
        assert_eq!(faces.len(), 1, "{name}");
        assert_eq!(faces[0].family, "Ljos Symbols Test", "{name}");
    }
}

#[test]
fn a_file_cut_short_is_corrupt_and_changes_nothing() {
    for (name, format) in [
        ("LjosSymbolsTest-Truncated.woff2", "WOFF2"),
        ("LjosSymbolsTest-Truncated.woff", "WOFF"),
    ] {
        let mut e = TextEngine::new();
        let before = e.generation();
        let err = e.register_font_faces(fixture(name)).unwrap_err();
        assert!(
            matches!(err, FontError::Corrupt(f) if f == format),
            "{name}: {err:?}"
        );
        assert_eq!(err.to_string(), format!("the {format} data is corrupt"));
        assert_eq!(e.generation(), before, "{name}");
        assert!(!e.has_family("Ljos Symbols Test"), "{name}");
    }
}

#[test]
fn a_signature_alone_is_corrupt() {
    let mut e = TextEngine::new();
    for (bytes, format) in [(&b"wOF2"[..], "WOFF2"), (&b"wOFF"[..], "WOFF")] {
        let err = e.register_font(bytes.to_vec()).unwrap_err();
        assert!(
            matches!(err, FontError::Corrupt(f) if f == format),
            "{err:?}"
        );
    }
}

/// A WOFF whose one table says it unpacks to 4 GiB, in a file of 64 bytes.
fn woff_claiming_4_gib() -> Vec<u8> {
    let mut b = fixture("LjosSymbolsTest-Regular.woff");
    b[12..14].copy_from_slice(&1u16.to_be_bytes());
    b[44 + 12..44 + 16].copy_from_slice(&u32::MAX.to_be_bytes());
    b.truncate(64);
    b
}

/// A WOFF2 whose header says the font it holds is 4 GiB.
fn woff2_claiming_4_gib() -> Vec<u8> {
    let mut b = fixture("LjosSymbolsTest-Regular.woff2");
    b[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    b
}

#[test]
fn a_woff2_or_woff_that_claims_more_than_any_font_is_too_large() {
    for (bytes, format) in [
        (woff_claiming_4_gib(), "WOFF"),
        (woff2_claiming_4_gib(), "WOFF2"),
    ] {
        let err = TextEngine::new().register_font(bytes).unwrap_err();
        assert!(
            matches!(err, FontError::TooLarge(f) if f == format),
            "{format}: {err:?}"
        );
        assert_eq!(
            err.to_string(),
            format!("the {format} data unpacks to more than 134217728 bytes")
        );
    }
}

// Every prefix and every single changed byte of each fixture: the decoder
// reads files from anywhere (a FontLoader's URL), so none may panic it.
#[test]
fn no_cut_or_changed_byte_panics() {
    let mut e = TextEngine::new();
    for name in [
        "LjosSymbolsTest-Regular.woff2",
        "LjosSymbolsTest-Regular.woff",
    ] {
        let good = fixture(name);
        for len in 0..good.len() {
            let _ = e.register_font(good[..len].to_vec());
        }
        for at in 0..good.len() {
            for flip in [0x01, 0x80, 0xff] {
                let mut bad = good.clone();
                bad[at] ^= flip;
                let _ = e.register_font(bad);
            }
        }
    }
}
