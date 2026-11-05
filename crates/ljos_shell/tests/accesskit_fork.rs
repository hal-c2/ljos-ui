//! The AccessKit adapter the shell holds is Ljós's fork, with the fixes
//! vendor/ACCESSKIT.md lists, and not the published one egui-winit would
//! bring: asks cargo what a build of the shell links, as `no_eframe.rs`
//! does. (A host with an eframe application of its own gets eframe's
//! adapter in eframe's windows, the published one.)

mod common;

use common::linked;

#[test]
fn the_shell_links_the_accesskit_fork_and_not_the_published_adapter() {
    for features in [&[][..], &["dev"], &["mcp"]] {
        let crates = linked(features);
        let has = |name: &str| crates.iter().any(|c| c == name);
        assert!(has("ljos_accesskit_winit"), "{features:?}: {crates:?}");
        if cfg!(target_os = "linux") {
            assert!(has("ljos_accesskit_unix"), "{features:?}: {crates:?}");
            assert!(
                has("ljos_accesskit_atspi_common"),
                "{features:?}: {crates:?}"
            );
        }
        for published in [
            "accesskit_winit",
            "accesskit_unix",
            "accesskit_atspi_common",
        ] {
            assert!(
                !has(published),
                "ljos_shell links the published {published} with the features {features:?}"
            );
        }
    }
}
