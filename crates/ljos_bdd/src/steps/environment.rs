//! What the application starts with, and what the system says while it
//! runs: styles, locales, settings, clocks, fonts, the network, the language.

use cucumber::{given, when};
use gherkin::Step;

use crate::backend::SystemSetting;
use crate::world::LjosWorld;

use super::{StepResult, docstring};

#[given(regex = r#"^the system clock reads "([^"]+)"$"#)]
fn wall_clock_reads(w: &mut LjosWorld, time: String) -> StepResult {
    let ms = ljos_runtime::parse_rfc3339_ms(&time)
        .ok_or_else(|| format!("{time:?} is not an RFC 3339 time such as 2026-10-11T17:57:09Z"))?;
    w.each(|b| b.environment()?.wall_clock(ms))
}

// On in a debug build already, unless `LJOS_TIER1_CHECK` says otherwise:
// the step makes a scenario that is about the check not depend on either.
#[given(regex = r"^the tier 1 cross-check is (on|off)$")]
fn tier1_check_is(w: &mut LjosWorld, on: String) -> StepResult {
    w.each(|b| b.environment()?.tier1_check(on == "on"))
}

// The time limits on JavaScript then do not depend on the machine: a poll
// of the interrupt is ten milliseconds, and native work takes what it is said
// to take.
#[given(
    regex = r"^JavaScript is timed by a clock on which native work a script asks for takes (\d+) seconds$"
)]
fn js_clock(w: &mut LjosWorld, secs: u64) -> StepResult {
    w.each(|b| {
        b.environment()?
            .js_clock(std::time::Duration::from_secs(secs))
    })
}

#[given(
    regex = r#"^the application uses the key bindings of "(Linux|GNOME|KDE Plasma|Windows|macOS)"$"#
)]
fn key_bindings(w: &mut LjosWorld, platform: String) -> StepResult {
    use ljos_quick::keys::Platform;
    let platform = match platform.as_str() {
        "GNOME" => Platform::Gnome,
        "KDE Plasma" => Platform::Kde,
        "Windows" => Platform::Windows,
        "macOS" => Platform::MacOs,
        _ => Platform::Linux,
    };
    w.each(|b| b.environment()?.key_bindings(platform))
}

#[given("the application uses the fonts installed on the system")]
fn system_fonts(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.environment()?.system_fonts())
}

#[when(regex = r#"^the UI language is set to "([^"]*)"$"#)]
fn ui_language_set(w: &mut LjosWorld, language: String) -> StepResult {
    w.each(|b| b.environment()?.set_ui_language(&language))
}

// The table has an attribute and its value on each row.
#[when("the application font is set to:")]
fn application_font_set(w: &mut LjosWorld, step: &Step) -> StepResult {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| "the step needs a table of attributes and values".to_owned())?;
    let attrs: Vec<(String, String)> = table
        .rows
        .iter()
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect();
    w.each(|b| b.environment()?.set_application_font(Some(&attrs)))
}

#[when("the application font is set back to the default")]
fn application_font_reset(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.environment()?.set_application_font(None))
}

// What the system prefers, as a scenario writes it.
fn system_setting(setting: &str, value: &str) -> Result<SystemSetting, String> {
    let flag = || match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{value} is not true or false")),
    };
    let word = value.trim_matches('"');
    match (setting, word) {
        ("color scheme", "dark") => Ok(SystemSetting::DarkScheme(true)),
        ("color scheme", "light") => Ok(SystemSetting::DarkScheme(false)),
        ("contrast", "high") => Ok(SystemSetting::HighContrast(true)),
        ("contrast", "normal") => Ok(SystemSetting::HighContrast(false)),
        ("reduced motion", _) => flag().map(SystemSetting::ReducedMotion),
        ("screen reader", _) => flag().map(SystemSetting::ScreenReader),
        ("font scale", _) => value
            .parse()
            .map(SystemSetting::FontScale)
            .map_err(|_| format!("{value} is not a number")),
        _ => Err(format!("{value} is not a {setting:?} the system has")),
    }
}

/// `LANG=de_DE.UTF-8 LC_NUMERIC=fr_FR.UTF-8`: the program's locale
/// variables, and no others; empty for none at all.
#[given(regex = r#"^the application starts with the locale environment "([^"]*)"$"#)]
fn locale_environment(w: &mut LjosWorld, vars: String) -> StepResult {
    let vars: Vec<(String, String)> = vars
        .split_whitespace()
        .map(|kv| {
            kv.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .ok_or_else(|| format!("{kv} is not NAME=value"))
        })
        .collect::<Result<_, _>>()?;
    w.each(|b| b.environment()?.locale_environment(&vars))
}

/// The text of the file `QT_QUICK_CONTROLS_CONF` names.
#[given(regex = r#"^the application starts with the controls configuration:$"#)]
fn controls_configuration(w: &mut LjosWorld, step: &Step) -> StepResult {
    let text = docstring(step)?;
    w.each(|b| b.environment()?.controls_configuration(&text))
}

#[given(regex = r#"^the application starts with "(QT_QUICK_CONTROLS_[A-Z_]+)" set to "([^"]*)"$"#)]
fn controls_variable(w: &mut LjosWorld, name: String, value: String) -> StepResult {
    w.each(|b| b.environment()?.controls_variable(&name, &value))
}

#[given(regex = r#"^the application starts with the controls style "([^"]+)"$"#)]
fn controls_style(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.environment()?.controls_style(&name))
}

#[given(regex = r#"^the application starts with "-style ([^"]+)" on its command line$"#)]
fn command_line_style(w: &mut LjosWorld, name: String) -> StepResult {
    w.each(|b| b.environment()?.command_line_style(&name))
}

#[given(regex = r#"^the system setting "([^"]+)" is (.+)$"#)]
fn system_setting_is(w: &mut LjosWorld, setting: String, value: String) -> StepResult {
    let setting = system_setting(&setting, &value)?;
    w.each(|b| b.environment()?.system_setting(setting))
}

#[when(regex = r#"^the system setting "([^"]+)" changes to (.+)$"#)]
fn system_setting_changes(w: &mut LjosWorld, setting: String, value: String) -> StepResult {
    let setting = system_setting(&setting, &value)?;
    w.each(|b| b.environment()?.system_setting(setting))
}

#[when(regex = r#"^the application sets the system setting "([^"]+)" to (.+)$"#)]
fn app_setting(w: &mut LjosWorld, setting: String, value: String) -> StepResult {
    let setting = system_setting(&setting, &value)?;
    w.each(|b| b.environment()?.app_setting(setting))
}

// What the system says of its network, as a scenario writes it: how far it
// reaches, then what carries it and anything else it says, comma-separated.
fn network_status(said: &str) -> Result<ljos_platform::NetworkStatus, String> {
    use ljos_platform::{Reachability as R, TransportMedium as M};
    let mut parts = said.split(',').map(str::trim);
    let mut status = ljos_platform::NetworkStatus {
        reachability: match parts.next().unwrap_or("") {
            "unknown" => R::Unknown,
            "disconnected" => R::Disconnected,
            "local" => R::Local,
            "site" => R::Site,
            "online" => R::Online,
            other => return Err(format!("{other:?} is not a reachability")),
        },
        ..Default::default()
    };
    for part in parts {
        match part {
            "ethernet" => status.transport_medium = M::Ethernet,
            "cellular" => status.transport_medium = M::Cellular,
            "wifi" => status.transport_medium = M::WiFi,
            "bluetooth" => status.transport_medium = M::Bluetooth,
            "behind a captive portal" => status.behind_captive_portal = true,
            "metered" => status.metered = true,
            other => return Err(format!("{other:?} is not something a network is")),
        }
    }
    Ok(status)
}

#[given(regex = r#"^the system's network is "([^"]+)"$"#)]
fn network_is(w: &mut LjosWorld, said: String) -> StepResult {
    let status = network_status(&said)?;
    w.each(|b| b.environment()?.network(status))
}

#[when(regex = r#"^the system's network changes to "([^"]+)"$"#)]
fn network_changes(w: &mut LjosWorld, said: String) -> StepResult {
    let status = network_status(&said)?;
    w.each(|b| b.environment()?.network(status))
}

#[given(regex = r#"^the desktop setting "([^"]+)" "([^"]+)" is (.+)$"#)]
fn desktop_setting_is(
    w: &mut LjosWorld,
    namespace: String,
    key: String,
    value: String,
) -> StepResult {
    let value = setting_value(&value)?;
    w.each(|b| {
        b.environment()?
            .desktop_setting(&namespace, &key, value.clone())
    })
}

#[when(regex = r#"^the desktop setting "([^"]+)" "([^"]+)" changes to (.+)$"#)]
fn desktop_setting_changes(
    w: &mut LjosWorld,
    namespace: String,
    key: String,
    value: String,
) -> StepResult {
    let value = setting_value(&value)?;
    w.each(|b| {
        b.environment()?
            .desktop_setting(&namespace, &key, value.clone())
    })
}

/// A value as the portal sends it: `true`, `uint32 1`, `1.25` (a double),
/// `48` (an int32), `"reduce"`.
fn setting_value(value: &str) -> Result<ljos_a11y::SettingValue, String> {
    use ljos_a11y::SettingValue as V;
    let bad = || format!("{value} is not a setting's value");
    let value = value.trim();
    Ok(match value {
        "true" => V::Bool(true),
        "false" => V::Bool(false),
        _ if value.starts_with('"') => V::Text(
            value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or_else(bad)?
                .to_owned(),
        ),
        _ => match value.strip_prefix("uint32 ") {
            Some(n) => V::Uint(n.parse().map_err(|_| bad())?),
            None if value.contains('.') => V::Double(value.parse().map_err(|_| bad())?),
            None => V::Int(value.parse().map_err(|_| bad())?),
        },
    })
}
