//! The cameras `LjosCamera.list` names, one `id\tfacing` line each. Apart
//! from the JNI that fetches them so it runs, and is tested, on every host.

use crate::camera::{CameraInfo, CameraPosition};

/// The cameras of `LjosCamera.list`'s lines; the first that faces back is
/// the default, else the first.
pub(crate) fn parse(listed: &str) -> Vec<CameraInfo> {
    let mut found: Vec<CameraInfo> = listed
        .lines()
        .filter_map(|line| {
            let (id, facing) = line.split_once('\t')?;
            let (position, description) = match facing {
                "back" => (CameraPosition::Back, "Back camera"),
                "front" => (CameraPosition::Front, "Front camera"),
                _ => (CameraPosition::Unspecified, "External camera"),
            };
            Some(CameraInfo {
                id: id.to_owned(),
                description: description.to_owned(),
                position,
                is_default: false,
            })
        })
        .collect();
    // Two cameras that face the same way are told apart by their ids. The
    // names are counted as listed: counting the renamed ones would leave the
    // last of each group its bare name.
    let base: Vec<String> = found.iter().map(|c| c.description.clone()).collect();
    for (camera, name) in found.iter_mut().zip(&base) {
        if base.iter().filter(|b| *b == name).count() > 1 {
            camera.description = format!("{name} {}", camera.id);
        }
    }
    let default = found
        .iter()
        .position(|c| c.position == CameraPosition::Back)
        .unwrap_or(0);
    if let Some(c) = found.get_mut(default) {
        c.is_default = true;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(found: &[CameraInfo]) -> Vec<&str> {
        found.iter().map(|c| c.description.as_str()).collect()
    }

    #[test]
    fn the_back_camera_is_the_default_and_alike_ones_are_told_apart() {
        let found = parse("0\tfront\n1\tback\n2\tback\n");
        assert_eq!(
            names(&found),
            ["Front camera", "Back camera 1", "Back camera 2"]
        );
        assert_eq!(found.iter().position(|c| c.is_default), Some(1));
        assert!(parse("").is_empty());
    }

    #[test]
    fn every_one_of_three_back_cameras_is_named_by_its_id() {
        let found = parse("0\tback\n2\tback\n3\tback\n1\tfront\n");
        assert_eq!(
            names(&found),
            [
                "Back camera 0",
                "Back camera 2",
                "Back camera 3",
                "Front camera"
            ]
        );
        assert_eq!(found.iter().position(|c| c.is_default), Some(0));
    }

    #[test]
    fn without_a_back_camera_the_first_is_the_default() {
        let found = parse("5\tfront\n7\texternal\n");
        assert_eq!(names(&found), ["Front camera", "External camera"]);
        assert_eq!(found.iter().position(|c| c.is_default), Some(0));
    }
}
