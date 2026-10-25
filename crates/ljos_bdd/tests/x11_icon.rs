//! The application's icon reaches the window manager: `ljos run --icon`
//! on the desktop sandbox's X server (its XWayland) gives each window the
//! picture as `_NET_WM_ICON`, which `xprop` reads back. The other
//! platforms' icons (the Dock's, Windows' taskbar, Wayland's
//! `xdg-toplevel-icon-v1`) have no window manager here to ask; the
//! sandbox's sway does not offer the Wayland protocol.
//!
//! It needs the sandbox the `@desktop` scenarios run on, `xprop` and
//! `xdotool`, and says so and passes without them:
//!
//! ```sh
//! cargo build -p ljos_cli
//! LJOS_CUA_HOME=target/cua-features scripts/cua-sandbox.sh env > target/features/desktop.env
//! LJOS_BDD_DESKTOP=target/features/desktop.env cargo test -p ljos_bdd --test x11_icon
//! ```

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ljos_bdd::backend::desktop;

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"),
        PathBuf::from,
    )
}

fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(rgba).unwrap();
    writer.finish().unwrap();
    out
}

fn missing() -> Option<String> {
    if let Some(why) = desktop::unavailable() {
        return Some(why);
    }
    for tool in ["xprop", "xdotool"] {
        let found = Command::new(tool)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();
        if !found {
            return Some(format!("no {tool}"));
        }
    }
    let ljos = target_dir().join("debug/ljos");
    (!ljos.is_file()).then(|| format!("{} is not built (cargo build -p ljos_cli)", ljos.display()))
}

/// What `xprop` says of `_NET_WM_ICON` on the windows of `pid`, once one
/// has it.
fn icon_of(env: &BTreeMap<String, String>, pid: u32, deadline: Instant) -> Option<Vec<u64>> {
    while Instant::now() < deadline {
        let windows = Command::new("xdotool")
            .args(["search", "--pid", &pid.to_string()])
            .envs(env)
            .output()
            .ok()?;
        for window in String::from_utf8_lossy(&windows.stdout).split_whitespace() {
            let said = Command::new("xprop")
                .args(["-id", window, "-f", "_NET_WM_ICON", "32c", " = $0+"])
                .arg("_NET_WM_ICON")
                .envs(env)
                .output()
                .ok()?;
            let said = String::from_utf8_lossy(&said.stdout);
            if let Some((_, values)) = said.split_once(" = ") {
                return Some(
                    values
                        .split(',')
                        .filter_map(|v| v.trim().parse().ok())
                        .collect(),
                );
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

#[test]
fn the_window_manager_is_given_the_applications_icon() {
    if let Some(why) = missing() {
        eprintln!("skipped: {why}");
        return;
    }
    let file = std::env::var_os("LJOS_BDD_DESKTOP").expect("checked above");
    let env: BTreeMap<String, String> = std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .filter(|(k, _)| k != "WAYLAND_DISPLAY")
        .collect();
    assert!(env.contains_key("DISPLAY"), "the sandbox has no X server");

    let features = target_dir().join("features");
    std::fs::create_dir_all(&features).unwrap();
    let dir = tempfile::Builder::new()
        .prefix("x11-icon-")
        .tempdir_in(features)
        .unwrap();
    // Two opaque red pixels, then two blue ones half seen.
    let row = [
        255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 128, 0, 0, 255, 128,
    ];
    std::fs::write(dir.path().join("icon.png"), png(4, 2, &[row, row].concat())).unwrap();
    std::fs::write(
        dir.path().join("Main.qml"),
        "import QtQuick\nWindow { width: 120; height: 80; visible: true }\n",
    )
    .unwrap();

    let mut app = Command::new(target_dir().join("debug/ljos"))
        .current_dir(dir.path())
        .args(["run", "Main.qml", "--icon", "icon.png"])
        // An X11 client of the sandbox's XWayland, nothing of the
        // developer's session.
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("AT_SPI_BUS_ADDRESS")
        .envs(&env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let icon = icon_of(&env, app.id(), Instant::now() + Duration::from_secs(60));
    let _ = app.kill();
    let _ = app.wait();

    // Width, height, then ARGB of each pixel, not premultiplied.
    let (red, blue) = (0xffff_0000, 0x8000_00ff);
    assert_eq!(
        icon.expect("a window with an icon"),
        [4, 2, red, red, blue, blue, red, red, blue, blue]
    );
}
