//! A scrolled clip looks the same whether the painter moved its pixels on
//! screen or drew it again (`LJOS_SCROLL_COPIES=off`): the same scrolls in
//! two runs on the desktop sandbox give the same window.
//!
//! Where the pixels were moved they are, pixel for pixel, those of the
//! frame before, moved by the scroll. Against the run that draws them again
//! each channel is within one level: a rasteriser need not give a smoothed
//! edge the same coverage at another place (GL promises the same pixels
//! only for the same place), and llvmpipe gives a circle drawn eleven pixels
//! lower a few edge pixels one level apart. A copy put in the wrong place,
//! a strip left undrawn or rows turned upside down are tens of levels off,
//! and noise by place on screen a level off across the whole clip.
//!
//! The scenario `features/window/scroll-pixels.feature` checks a few
//! pixels of the same thing against what the rows are; this checks every
//! pixel of every step, text and smoothed edges included.
//!
//! It needs the sandbox the `@desktop` scenarios run on, and says so and
//! passes without one:
//!
//! ```sh
//! LJOS_CUA_HOME=target/cua-features scripts/cua-sandbox.sh env > target/features/desktop.env
//! LJOS_BDD_DESKTOP=target/features/desktop.env cargo test -p ljos_bdd --test scroll_copies
//! ```

use ljos_bdd::backend::desktop::{self, DesktopBackend};
use ljos_bdd::backend::{Backend, QmlFile};
use serde_json::{Value, json};

const QML: &str = r##"
import QtQuick
Rectangle {
    width: 320; height: 260; color: "#203040"
    ListView {
        objectName: "list"
        x: 10; y: 10; width: 140; height: 200
        clip: true
        model: 60
        delegate: Rectangle {
            width: 140; height: 27
            color: index % 2 ? "#f4f0e8" : "#d8e4f0"
            Rectangle {
                x: 4; y: 4; width: 19; height: 19; radius: 9.5
                color: Qt.hsla(index / 60, 0.7, 0.5, 1)
            }
            Text { x: 30; y: 5; text: "Row " + index + " of the list" }
        }
    }
    Flickable {
        objectName: "flick"
        x: 165; y: 25; width: 140; height: 210
        clip: true
        contentWidth: 140; contentHeight: 1200
        Column {
            Repeater {
                model: 24
                Rectangle {
                    width: 140; height: 50; radius: 12
                    border.width: 2; border.color: "#101010"
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.hsla(index / 24, 0.6, 0.6, 1) }
                        GradientStop { position: 1; color: "white" }
                    }
                    Text { anchors.centerIn: parent; text: "Card " + index }
                }
            }
        }
    }
}
"##;

/// The clips of the two views in the window's pixels: x, y, width, height.
const CLIPS: [(&str, [u32; 4]); 2] = [("list", [10, 10, 140, 200]), ("flick", [165, 25, 140, 210])];

/// The scrolls, one after another: small steps either way, a step of a
/// fraction of a pixel, one larger than the clip, and both views at once.
const STEPS: &[(&str, f64)] = &[
    ("list", 3.0),
    ("list", 10.0),
    ("list", 27.0),
    ("list", 41.0),
    ("list", 40.5),
    ("list", 17.0),
    ("list", 400.0),
    ("list", 389.0),
    ("flick", 6.0),
    ("flick", 55.0),
    ("flick", 49.0),
    ("list", 395.0),
    ("flick", 312.0),
    ("flick", 300.0),
];

struct Run {
    pictures: Vec<ljos_render::RgbaImage>,
    /// Whether the passes up to each picture moved a clip's pixels.
    moved: Vec<bool>,
}

fn run(copies: &str) -> Run {
    let mut backend = DesktopBackend::new().with_env("LJOS_SCROLL_COPIES", copies);
    let files = [QmlFile {
        name: "main.qml".into(),
        source: QML.into(),
    }];
    backend
        .load(&files, "main.qml")
        .expect("the application starts");
    let mut pictures = vec![picture(&mut backend)];
    let mut was = moved(&mut backend);
    let mut moves = vec![was > 0];
    for &(view, y) in STEPS {
        backend
            .set(view, "contentY", &json!(y))
            .expect("the view scrolls");
        pictures.push(picture(&mut backend));
        let now = moved(&mut backend);
        moves.push(now > was);
        was = now;
    }
    Run {
        pictures,
        moved: moves,
    }
}

/// The passes so far that moved a clip's pixels.
fn moved(backend: &mut DesktopBackend) -> u64 {
    let metrics = backend.metrics().expect("the inspector answers");
    metrics["paint"]["moved"].as_u64().unwrap_or_else(|| {
        panic!("app_metrics says nothing of moved passes: {metrics}");
    })
}

fn picture(backend: &mut DesktopBackend) -> ljos_render::RgbaImage {
    let png = backend.grab().expect("a picture of the window");
    ljos_render::RgbaImage::from_png(&png).expect("a PNG")
}

fn save(name: &str, picture: &ljos_render::RgbaImage) -> String {
    let dir = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(
            || std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"),
            std::path::PathBuf::from,
        )
        .join("features/scroll-copies");
    std::fs::create_dir_all(&dir).expect("the pictures' directory");
    let path = dir.join(name);
    std::fs::write(&path, picture.to_png().expect("encoded")).expect("written");
    path.display().to_string()
}

/// Where `now` is `before` moved down by `dy` inside `clip`, the pixels
/// of the clip that were in sight in both that differ.
fn not_moved(
    before: &ljos_render::RgbaImage,
    now: &ljos_render::RgbaImage,
    [x0, y0, w, h]: [u32; 4],
    dy: i64,
) -> usize {
    let mut n = 0;
    for y in y0..y0 + h {
        let from = i64::from(y) - dy;
        let Ok(from) = u32::try_from(from) else {
            continue;
        };
        if from < y0 || from >= y0 + h {
            continue;
        }
        n += (x0..x0 + w)
            .filter(|&x| now.pixel(x, y) != before.pixel(x, from))
            .count();
    }
    n
}

#[test]
fn moved_pixels_are_the_pixels_drawn_again() {
    if let Some(why) = desktop::unavailable() {
        eprintln!("SKIPPED: {why}");
        return;
    }
    let on = run("on");
    let off = run("off");
    let moves = on.moved.iter().filter(|m| **m).count();
    assert!(
        moves > 0,
        "no pass moved a scrolled clip's pixels, so nothing was compared"
    );
    assert!(
        !off.moved.contains(&true),
        "LJOS_SCROLL_COPIES=off still moved pixels"
    );
    let mut wrong = Vec::new();
    let mut scrolled = std::collections::HashMap::new();
    let mut near = 0;
    for (step, (a, b)) in on.pictures.iter().zip(&off.pictures).enumerate() {
        assert_eq!(
            (a.width, a.height),
            (b.width, b.height),
            "the window changed size"
        );
        let what = step
            .checked_sub(1)
            .map_or(Value::from("the first frame"), |i| json!(STEPS[i]));
        if let Some(&(view, y)) = step.checked_sub(1).map(|i| &STEPS[i]) {
            let was = scrolled.insert(view, y).unwrap_or(0.0);
            let clip = CLIPS.iter().find(|(v, _)| *v == view).expect("a view").1;
            #[allow(clippy::cast_possible_truncation)]
            let dy = (was - y) as i64;
            // The first scroll of a clip draws it and keeps it, and one clip
            // is kept at a time: only the passes that moved pixels are held
            // to them. After a scroll by part of a pixel the shapes the
            // scene puts on whole pixels move by one more or less than the
            // rest, which are drawn again, so only a whole one is.
            #[allow(clippy::cast_precision_loss)]
            let whole = (was - y) == dy as f64;
            if on.moved[step] && whole {
                let n = not_moved(&on.pictures[step - 1], a, clip, dy);
                if n > 0 {
                    wrong.push(format!(
                        "after {what}: {n} pixels of the clip are not those of the frame before, moved by {dy}; {} and {}",
                        save(&format!("{step}-before.png"), &on.pictures[step - 1]),
                        save(&format!("{step}-moved.png"), a),
                    ));
                }
            }
        }
        let mut far = Vec::new();
        for y in 0..a.height {
            for x in 0..a.width {
                let (p, q) = (a.pixel(x, y), b.pixel(x, y));
                let off_by = p
                    .iter()
                    .zip(q)
                    .map(|(p, q)| p.abs_diff(q))
                    .max()
                    .unwrap_or(0);
                match off_by {
                    0 => {}
                    1 => near += 1,
                    _ => far.push((x, y, p, q)),
                }
            }
        }
        if let Some(&(x, y, p, q)) = far.first() {
            wrong.push(format!(
                "after {what}: {} pixels differ by more than a level, the first at {x}, {y} ({p:?} moved, {q:?} drawn); {} and {}",
                far.len(),
                save(&format!("{step}-moved.png"), a),
                save(&format!("{step}-drawn.png"), b),
            ));
        }
    }
    eprintln!(
        "{moves} of {} scrolls moved pixels; {near} pixels of {} frames are a level from those drawn again",
        STEPS.len(),
        on.pictures.len()
    );
    // A few edge pixels, not a picture: noise by place on screen (egui's
    // dithering) put a level on thousands of pixels of each moved clip.
    let compared: usize = on
        .pictures
        .iter()
        .map(|p| p.width as usize * p.height as usize)
        .sum();
    if near * 1000 > compared {
        wrong.push(format!(
            "{near} of {compared} pixels are a level from those drawn again: more than smoothed edges"
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
