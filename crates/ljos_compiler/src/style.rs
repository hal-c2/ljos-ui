//! The Controls style a program gets when it is started, as Qt chooses it
//! at run time from the environment, a configuration file and the platform.

use qml_front::project::ControlsStyle;

/// The style Qt's run-time selection gives a program started with these
/// variables on this system: `QT_QUICK_CONTROLS_STYLE`, else `Style` of
/// the `[Controls]` group of the file `QT_QUICK_CONTROLS_CONF` names, else
/// the [platform's](platform_style); the fallback style from
/// `QT_QUICK_CONTROLS_FALLBACK_STYLE` or `FallbackStyle` likewise. A
/// `qtquickcontrols2.conf` beside the files is not read, as `qml` does
/// not read it. A `-style` on the program's command line is the caller's
/// to put first.
pub fn controls_style(
    var: impl Fn(&str) -> Option<String>,
    read: impl Fn(&str) -> Option<String>,
) -> Option<ControlsStyle> {
    controls_style_on(std::env::consts::OS, var, read)
}

/// The style a program that chooses none has on `os` (as
/// `std::env::consts::OS` names it), as Qt documents the default of each
/// platform: Fusion on Linux, Material on Android. `None` is Basic, which
/// is also where Qt has a style of the platform's own (macOS, Windows,
/// iOS) that Ljós does not have.
// DECISION: the platform is that of the machine that compiles; a program
// compiled for another names its style.
pub fn platform_style(os: &str) -> Option<&'static str> {
    match os {
        "linux" => Some("Fusion"),
        "android" => Some("Material"),
        _ => None,
    }
}

/// [`controls_style`] as it chooses on `os`.
pub fn controls_style_on(
    os: &str,
    var: impl Fn(&str) -> Option<String>,
    read: impl Fn(&str) -> Option<String>,
) -> Option<ControlsStyle> {
    let conf = var("QT_QUICK_CONTROLS_CONF").and_then(|path| read(&path));
    let given =
        |value: Option<String>| value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let from_conf = |key: &str| given(conf.as_deref().and_then(|text| ini_value(text, key)));
    let name = given(var("QT_QUICK_CONTROLS_STYLE"))
        .or_else(|| from_conf("Style"))
        .or_else(|| platform_style(os).map(str::to_owned))?;
    let fallback =
        given(var("QT_QUICK_CONTROLS_FALLBACK_STYLE")).or_else(|| from_conf("FallbackStyle"));
    Some(ControlsStyle { name, fallback })
}

/// The value of `key` in the `[Controls]` group of an INI text.
fn ini_value(text: &str, key: &str) -> Option<String> {
    let mut in_controls = false;
    for line in text.lines().map(str::trim) {
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_controls = group.trim() == "Controls";
        } else if in_controls
            && let Some((k, v)) = line.split_once('=')
            && k.trim().eq_ignore_ascii_case(key)
        {
            return Some(v.trim().trim_matches('"').to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(vars: &[(&str, &str)], conf: &str) -> Option<ControlsStyle> {
        let var = |name: &str| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_owned())
        };
        // A platform whose default is Basic, so that what is not chosen
        // reads as `None`.
        controls_style_on("macos", var, |_| Some(conf.to_owned()))
    }

    fn named(name: &str, fallback: Option<&str>) -> Option<ControlsStyle> {
        Some(ControlsStyle {
            name: name.to_owned(),
            fallback: fallback.map(str::to_owned),
        })
    }

    /// The order Qt documents and qml6 shows: the variable, then the file.
    #[test]
    fn the_variable_comes_before_the_configuration_file() {
        let conf = "[Controls]\nStyle=Kit\nFallbackStyle=Material\n\n[Material]\nTheme=Dark\n";
        assert_eq!(style(&[], conf), None);
        let file = ("QT_QUICK_CONTROLS_CONF", "x.conf");
        assert_eq!(style(&[file], conf), named("Kit", Some("Material")));
        assert_eq!(
            style(&[file, ("QT_QUICK_CONTROLS_STYLE", "Basic")], conf),
            named("Basic", Some("Material"))
        );
        assert_eq!(
            style(
                &[
                    ("QT_QUICK_CONTROLS_STYLE", "Kit"),
                    ("QT_QUICK_CONTROLS_FALLBACK_STYLE", "Fusion")
                ],
                ""
            ),
            named("Kit", Some("Fusion"))
        );
        // Another group's `Style` is not the controls'.
        assert_eq!(style(&[file], "[Other]\nStyle=Kit\n"), None);
        assert_eq!(style(&[("QT_QUICK_CONTROLS_STYLE", " ")], ""), None);
    }

    /// Qt's default of each platform, behind every choice a program makes.
    #[test]
    fn a_program_that_chooses_nothing_has_its_platforms_style() {
        let none = |_: &str| None;
        let on = |os| controls_style_on(os, none, none);
        assert_eq!(on("linux"), named("Fusion", None));
        assert_eq!(on("android"), named("Material", None));
        for os in ["macos", "windows", "ios", "freebsd"] {
            assert_eq!(on(os), None, "{os}");
        }
        let var = |name: &str| (name == "QT_QUICK_CONTROLS_STYLE").then(|| "Basic".to_owned());
        assert_eq!(controls_style_on("linux", var, none), named("Basic", None));
        let var = |name: &str| (name == "QT_QUICK_CONTROLS_CONF").then(|| "x.conf".to_owned());
        let read = |_: &str| Some("[Controls]\nStyle=Universal\n".to_owned());
        assert_eq!(
            controls_style_on("linux", var, read),
            named("Universal", None)
        );
    }
}
