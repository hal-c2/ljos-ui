//! `ljostypes.json` says what `register` registers: the build script
//! compiles the QML against it.
//! `LJOS_TYPES=write cargo test --no-fail-fast` writes it.

#[test]
fn the_type_dump_file_is_current() {
    ljos::types::check_file("ljostypes.json".as_ref(), ljos_facade_aot::register).unwrap();
}
