//! The MCP inspector as an application asks for it (the `mcp` feature):
//! in code with `App::inspector`, or with no code through `LJOS_MCP` and
//! `LJOS_MCP_DIR`, as `ljos run --mcp` grants it.
#![cfg(feature = "mcp")]

use std::{ffi::OsStr, path::Path};

use ljos::App;
use ljos::mcp::{Cap, CapSet, Inspector, InspectorError};

#[test]
fn an_application_grants_capabilities_by_name() {
    let inspector = Inspector::parse("inspect,input").unwrap();
    assert_eq!(inspector.caps(), CapSet::of([Cap::Inspect, Cap::Input]));
}

#[test]
fn inspect_has_to_be_granted() {
    assert!(matches!(
        Inspector::parse("input"),
        Err(InspectorError::NoInspect)
    ));
}

#[test]
fn what_belongs_to_the_authoring_server_is_refused() {
    let err = Inspector::parse("inspect,fs").unwrap_err();
    assert!(matches!(err, InspectorError::NotForApplication(_)), "{err}");
    assert!(err.to_string().contains("fs"), "{err}");
}

#[test]
fn a_name_that_is_no_capability_is_refused() {
    assert!(matches!(
        Inspector::parse("inspect,everything"),
        Err(InspectorError::Caps(_))
    ));
}

#[test]
fn nothing_set_serves_nothing() {
    assert!(Inspector::from_vars(None, None).unwrap().is_none());
    assert!(
        Inspector::from_vars(Some(OsStr::new("")), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_variable_grants_what_it_names_and_the_token_goes_below_the_directory() {
    let dir = Path::new("/somewhere/app");
    let inspector = Inspector::from_vars(Some(OsStr::new("inspect,input")), Some(dir.as_os_str()))
        .unwrap()
        .expect("asked for");
    assert_eq!(inspector.caps(), CapSet::of([Cap::Inspect, Cap::Input]));
    let (options, access) = inspector.options().unwrap();
    assert!(options.token_file.starts_with(dir), "{options:?}");
    assert_eq!(access.caps, inspector.caps());
}

#[test]
fn the_builder_takes_the_inspector() {
    let app = App::new("Inspected").inspector(CapSet::of([Cap::Inspect]));
    assert!(format!("{app:?}").contains("inspector: true"), "{app:?}");
}
