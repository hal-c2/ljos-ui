//! `Image` and `BorderImage`: loading, the shared cache and its budget,
//! background decoding, and what reaches the display list.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread::ThreadId;

use common::*;
use ljos_engine::{Engine, LogLevel, ObjectId, Value};
use ljos_quick::{ImageStore, QuickScene, image_status};
use ljos_render::{DrawOp, ImageHandle, Insets, TileMode, kurbo};
use ljos_runtime::{Evictable, MemoryPressure};

// ---------------------------------------------------------------- fixtures

/// A PNG of one colour.
fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let data: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
        w.write_image_data(&data).unwrap();
    }
    out
}

/// A grey-scale PNG with an alpha channel, to go through the conversion.
fn grey_png(width: u32, height: u32, grey: u8, alpha: u8) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::GrayscaleAlpha);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let data: Vec<u8> = (0..width * height).flat_map(|_| [grey, alpha]).collect();
        w.write_image_data(&data).unwrap();
    }
    out
}

/// A JPEG of one colour.
fn jpeg(width: u16, height: u16, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    let data: Vec<u8> = (0..u32::from(width) * u32::from(height))
        .flat_map(|_| rgb)
        .collect();
    jpeg_encoder::Encoder::new(&mut out, 95)
        .encode(&data, width, height, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    out
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n =
            chunk.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// An engine whose store knows `red.png` (40×20), `blue.png` (60×40) and
/// `green.png` (10×10). A file compiled as `Main` resolves `red.png` to
/// just that.
fn pictures() -> (Engine, std::rc::Rc<ImageStore>) {
    let e = engine();
    let store = ImageStore::of(&e);
    store.register_resource("file:///red.png", png(40, 20, RED));
    store.register_resource("file:///blue.png", png(60, 40, BLUE));
    store.register_resource("file:///green.png", png(10, 10, GREEN));
    (e, store)
}

fn status(e: &Engine, o: ObjectId) -> i32 {
    num(e, o, "status") as i32
}

fn implicit(e: &Engine, o: ObjectId) -> (f64, f64) {
    (num(e, o, "implicitWidth"), num(e, o, "implicitHeight"))
}

fn source_size(e: &Engine, o: ObjectId) -> (f64, f64) {
    match get(e, o, "sourceSize") {
        Value::Size(s) => (s.width, s.height),
        other => panic!("sourceSize is {other:?}"),
    }
}

fn warnings(e: &Engine) -> Vec<String> {
    e.take_log()
        .into_iter()
        .filter(|l| l.level >= LogLevel::Warning)
        .map(|l| l.message)
        .collect()
}

/// The draw ops of `item`'s display list; empty when it has none.
fn ops(s: &QuickScene, item: ObjectId) -> Vec<DrawOp> {
    s.node_of(item)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
        .map(|l| l.ops().to_vec())
        .unwrap_or_default()
}

/// The picture `item` paints, if it paints one.
fn painted(s: &QuickScene, item: ObjectId) -> Option<ImageHandle> {
    ops(s, item).into_iter().find_map(|op| match op {
        DrawOp::Image { image, .. } | DrawOp::NinePatch { image, .. } => Some(image),
        _ => None,
    })
}

// ----------------------------------------------------------------- loading

#[test]
fn a_local_picture_is_ready_at_once_and_sizes_the_image() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "picture"; source: "red.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(num(&e, picture, "progress"), 1.0);
    assert_eq!(implicit(&e, picture), (40.0, 20.0));
    assert_eq!(rect(&e, picture), (0.0, 0.0, 40.0, 20.0));
    assert_eq!(source_size(&e, picture).0, 40.0);
    assert_eq!(source_size(&e, picture).1, 20.0);
    assert_eq!(num(&e, picture, "paintedWidth"), 40.0);
    assert_eq!(num(&e, picture, "paintedHeight"), 20.0);
    assert_eq!(store.decodes(), 1);
    assert!(warnings(&e).is_empty());
}

#[test]
fn the_ready_signals_come_in_qts_order() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    id: root
    property string events: ""
    Image {
        objectName: "picture"; source: "red.png"
        onProgressChanged: root.events += "progress "
        onImplicitWidthChanged: root.events += "implicitWidth "
        onStatusChanged: root.events += "status" + status + ":" + implicitWidth + ":" + progress + " "
        onSourceSizeChanged: root.events += "sourceSize "
    }
}
"#,
    );
    assert_eq!(
        get(&e, root, "events").to_js_string().as_str(),
        "progress implicitWidth status1:40:1 sourceSize "
    );
}

#[test]
fn a_missing_file_is_an_error_with_qts_warning() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "picture"; source: "file:///nonexistent/ljos/missing.png" }
    BorderImage { objectName: "frame"; source: "file:///nonexistent/ljos/missing.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    assert_eq!(status(&e, picture), image_status::ERROR);
    assert_eq!(num(&e, picture, "progress"), 0.0);
    assert_eq!(implicit(&e, picture), (0.0, 0.0));
    assert_eq!(
        warnings(&e),
        [
            "file:///Main.qml:5:5: QML BorderImage: Cannot open: file:///nonexistent/ljos/missing.png",
            "file:///Main.qml:4:5: QML Image: Cannot open: file:///nonexistent/ljos/missing.png",
        ]
    );
}

#[test]
fn a_file_on_disk_loads_through_its_url_and_as_a_bare_path() {
    let dir = std::env::temp_dir().join(format!("ljos-image-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("on disk.png");
    std::fs::write(&path, png(8, 6, GREEN)).unwrap();
    let e = engine();
    let url = format!("file://{}", path.to_string_lossy().replace(' ', "%20"));
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
Item {{
    Image {{ objectName: "by url"; source: "{url}" }}
}}
"#
        ),
    );
    assert_eq!(implicit(&e, find(&e, root, "by url")), (8.0, 6.0));
    // An item made in Rust has no file to resolve against: a path is a path.
    let native = make::<ljos_quick::Image>(&e, None);
    set(
        &e,
        native,
        "source",
        Value::Url(path.to_string_lossy().as_ref().into()),
    );
    assert_eq!(status(&e, native), image_status::READY);
    assert_eq!(implicit(&e, native), (8.0, 6.0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_resource_staged_as_a_file_is_read_when_it_is_shown() {
    let dir = std::env::temp_dir().join(format!("ljos-resource-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("knob.png");
    let e = engine();
    let store = ImageStore::of(&e);
    store.register_resource_file("qrc:/app/knob.png", &path);
    store.register_resource_file("qrc:/app/gone.png", dir.join("gone.png"));
    assert!(store.has_resource("qrc:/app/knob.png"));
    // Not there when it was staged: nothing was read then.
    std::fs::write(&path, png(8, 6, GREEN)).unwrap();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "knob"; source: "qrc:/app/knob.png" }
    Image { objectName: "gone"; source: "qrc:/app/gone.png" }
}
"#,
    );
    assert_eq!(implicit(&e, find(&e, root, "knob")), (8.0, 6.0));
    assert_eq!(status(&e, find(&e, root, "gone")), image_status::ERROR);
    assert_eq!(
        warnings(&e),
        ["file:///Main.qml:5:5: QML Image: Cannot open: qrc:/app/gone.png"]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn png_and_jpeg_are_told_apart_by_their_bytes() {
    let e = engine();
    let store = ImageStore::of(&e);
    // The names lie on purpose.
    store.register_resource("file:///photo.png", jpeg(24, 16, [200, 30, 30]));
    store.register_resource("file:///grey.jpg", grey_png(5, 7, 128, 255));
    store.register_resource("file:///letter.png", b"Dear reader,".to_vec());
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 100; height: 100
    Image { objectName: "photo"; source: "photo.png" }
    Image { objectName: "grey"; source: "grey.jpg" }
    Image { objectName: "letter"; source: "letter.png" }
}
"#,
    );
    let (photo, grey, letter) = (
        find(&e, root, "photo"),
        find(&e, root, "grey"),
        find(&e, root, "letter"),
    );
    assert_eq!(implicit(&e, photo), (24.0, 16.0));
    assert_eq!(implicit(&e, grey), (5.0, 7.0));
    assert_eq!(status(&e, letter), image_status::ERROR);
    assert_eq!(
        warnings(&e),
        [
            "file:///Main.qml:7:5: QML Image: Error decoding: file:///letter.png: Unsupported image format"
        ]
    );
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    let px = painted(&s, photo).expect("the photo paints").pixel(12, 8);
    assert!(px[0] > 180 && px[1] < 60 && px[2] < 60, "{px:?}");
    assert_eq!(
        painted(&s, grey).expect("the grey paints").pixel(0, 0),
        [128, 128, 128, 255]
    );
    assert!(painted(&s, letter).is_none());
}

#[test]
fn a_data_url_carries_the_picture_and_arrives_later() {
    let e = engine();
    let store = ImageStore::of(&e);
    store.set_held(true);
    let url = format!("data:image/png;base64,{}", base64(&png(6, 4, BLUE)));
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
Item {{
    Image {{ objectName: "picture"; source: "{url}" }}
}}
"#
        ),
    );
    let picture = find(&e, root, "picture");
    // Not a local file, so not read on the spot (as in Qt).
    assert_eq!(status(&e, picture), image_status::LOADING);
    store.finish(&e);
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(implicit(&e, picture), (6.0, 4.0));
}

#[test]
fn an_unknown_scheme_fails_with_qts_words() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "picture"; source: "nope:/a.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    ImageStore::of(&e).finish(&e);
    assert_eq!(status(&e, picture), image_status::ERROR);
    assert_eq!(
        warnings(&e),
        ["file:///Main.qml:4:5: QML Image: Protocol \"nope\" is unknown"]
    );
}

// ----------------------------------------------------------------- network

/// A web server on loopback that answers `/red.png` (30×20) and 404 for
/// anything else, one request per connection, until the test ends.
fn web_server() -> String {
    use std::io::{BufRead as _, BufReader, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let red = png(30, 20, RED);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
            }
            let (status, body) = if line.starts_with("GET /red.png ") {
                ("200 OK", red.clone())
            } else {
                ("404 Not Found", Vec::new())
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    address
}

fn networked() -> Engine {
    let e = engine();
    ljos_platform::Platform::of(&e).set_fetcher(Arc::new(ljos_platform::HttpFetcher::new()));
    e
}

#[test]
fn a_network_picture_loads_in_the_background_even_when_not_asynchronous() {
    let server = web_server();
    let e = networked();
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
Item {{
    Image {{ objectName: "picture"; asynchronous: false; source: "{server}/red.png" }}
}}
"#
        ),
    );
    let picture = find(&e, root, "picture");
    assert_eq!(status(&e, picture), image_status::LOADING);
    assert_eq!(num(&e, picture, "progress"), 0.0);
    ImageStore::of(&e).finish(&e);
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(num(&e, picture, "progress"), 1.0);
    assert_eq!(implicit(&e, picture), (30.0, 20.0));
    assert_eq!(source_size(&e, picture), (30.0, 20.0));
    assert!(warnings(&e).is_empty());
}

#[test]
fn a_network_picture_the_server_lacks_fails_with_qts_words() {
    let server = web_server();
    let e = networked();
    let root = load(
        &e,
        &format!(
            r#"
import QtQuick
Item {{
    Image {{ objectName: "picture"; source: "{server}/gone.png" }}
}}
"#
        ),
    );
    let picture = find(&e, root, "picture");
    ImageStore::of(&e).finish(&e);
    assert_eq!(status(&e, picture), image_status::ERROR);
    assert_eq!(
        warnings(&e),
        [format!(
            "file:///Main.qml:4:5: QML Image: Error transferring {server}/gone.png - server replied: Not Found"
        )]
    );
}

#[test]
fn without_a_network_a_network_picture_is_an_error() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "picture"; source: "https://example.invalid/a.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    assert_eq!(status(&e, picture), image_status::LOADING);
    ImageStore::of(&e).finish(&e);
    assert_eq!(status(&e, picture), image_status::ERROR);
    assert_eq!(
        warnings(&e),
        [
            "file:///Main.qml:4:5: QML Image: this application cannot fetch https://example.invalid/a.png: it has no network access"
        ]
    );
}

/// A fetcher that answers only when the test lets it.
struct Gate {
    open: std::sync::Mutex<mpsc::Receiver<()>>,
    bytes: Vec<u8>,
}

impl ljos_platform::Fetcher for Gate {
    fn fetch(&self, _url: &str) -> Result<Vec<u8>, ljos_platform::FetchError> {
        let _ = self.open.lock().unwrap().recv();
        Ok(self.bytes.clone())
    }
}

#[test]
fn a_network_picture_left_behind_while_on_its_way_is_ignored() {
    let (e, store) = pictures();
    let (open, gate) = mpsc::channel();
    ljos_platform::Platform::of(&e).set_fetcher(Arc::new(Gate {
        open: std::sync::Mutex::new(gate),
        bytes: png(30, 20, GREEN),
    }));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "picture"; source: "http://example.invalid/a.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    assert_eq!(status(&e, picture), image_status::LOADING);
    set(&e, picture, "source", Value::from("blue.png"));
    assert_eq!(status(&e, picture), image_status::READY);
    open.send(()).unwrap();
    store.finish(&e);
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(implicit(&e, picture), (60.0, 40.0));
    assert!(warnings(&e).is_empty());
}

#[test]
fn clearing_and_changing_the_source() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "picture"; source: "red.png"; width: 100 }
}
"#,
    );
    let picture = find(&e, root, "picture");
    set(&e, picture, "source", Value::Url("blue.png".into()));
    assert_eq!(implicit(&e, picture), (60.0, 40.0));
    // A width of its own stays; the height follows the picture.
    assert_eq!(rect(&e, picture), (0.0, 0.0, 100.0, 40.0));
    set(&e, picture, "source", Value::Url("".into()));
    assert_eq!(status(&e, picture), image_status::NULL);
    assert_eq!(num(&e, picture, "progress"), 0.0);
    assert_eq!(implicit(&e, picture), (0.0, 0.0));
    assert_eq!(source_size(&e, picture).0, 0.0);
}

// ------------------------------------------------------------- source size

#[test]
fn a_source_size_is_kept_and_decides_the_decoded_size() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "half"; source: "blue.png"; sourceSize.width: 30 }
    Image { objectName: "whole"; source: "blue.png" }
    Image {
        objectName: "cover"; source: "blue.png"
        fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(20, 120)
    }
    Image { objectName: "part"; source: "blue.png"; sourceClipRect: Qt.rect(10, 5, 20, 12) }
}
"#,
    );
    let half = find(&e, root, "half");
    assert_eq!(implicit(&e, half), (30.0, 20.0));
    // What was asked reads back, not what was decoded.
    assert_eq!(source_size(&e, half).0, 30.0);
    assert_eq!(source_size(&e, half).1, 0.0);
    assert_eq!(implicit(&e, find(&e, root, "whole")), (60.0, 40.0));
    assert_eq!(implicit(&e, find(&e, root, "cover")), (180.0, 120.0));
    let part = find(&e, root, "part");
    assert_eq!(implicit(&e, part), (20.0, 12.0));
    assert_eq!(source_size(&e, part).0, 20.0);
    // Four requests, four pictures: sizes do not share pixels.
    assert_eq!((store.decodes(), store.len()), (4, 4));
}

#[test]
fn a_source_size_set_later_loads_again_and_can_be_taken_back() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "picture"; source: "blue.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    set(&e, picture, "sourceSize", Value::size(30.0, 0.0));
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(implicit(&e, picture), (30.0, 20.0));
    assert_eq!(source_size(&e, picture).1, 0.0);
    // The fill mode decides how both sides are met.
    set(&e, picture, "sourceSize", Value::size(30.0, 10.0));
    assert_eq!(implicit(&e, picture), (15.0, 10.0));
    set(&e, picture, "fillMode", Value::Int(1));
    assert_eq!(implicit(&e, picture), (30.0, 20.0));
    // An empty size asks for nothing; the property reports again.
    set(&e, picture, "sourceSize", Value::size(0.0, 0.0));
    assert_eq!(implicit(&e, picture), (60.0, 40.0));
    assert_eq!(source_size(&e, picture).0, 60.0);
    // Asking for exactly what it reports is a request like any other.
    set(&e, picture, "source", Value::Url("red.png".into()));
    assert_eq!(source_size(&e, picture).0, 40.0);
    set(&e, picture, "sourceSize", Value::size(20.0, 0.0));
    set(&e, picture, "sourceSize", Value::size(40.0, 20.0));
    set(&e, picture, "source", Value::Url("blue.png".into()));
    // Kept in shape (the fill mode set above), the larger scale wins.
    assert_eq!(
        implicit(&e, picture),
        (40.0, 27.0),
        "40 by 20 was asked for"
    );
}

// ------------------------------------------------------------------- cache

#[test]
fn two_images_with_one_source_decode_once() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "a"; source: "red.png" }
    Image { objectName: "b"; source: "red.png"; x: 50; fillMode: Image.Tile }
}
"#,
    );
    assert_eq!((store.decodes(), store.len()), (1, 1));
    assert_eq!(store.bytes(), 40 * 20 * 4);
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    let a = painted(&s, find(&e, root, "a")).unwrap();
    let b = painted(&s, find(&e, root, "b")).unwrap();
    assert_eq!(a.id(), b.id(), "the same pixels");
    assert_eq!(store.decodes(), 1);
}

#[test]
fn an_image_that_does_not_cache_keeps_its_own_pixels() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "own"; source: "red.png"; cache: false }
    Image { objectName: "shared"; source: "red.png"; x: 50 }
}
"#,
    );
    // The first left nothing in the cache for the second to find.
    assert_eq!((store.decodes(), store.len()), (2, 1));
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    let own = painted(&s, find(&e, root, "own")).unwrap();
    let shared = painted(&s, find(&e, root, "shared")).unwrap();
    assert_ne!(own.id(), shared.id());
    // Its pixels are not the store's to drop.
    store.trim(MemoryPressure::Critical);
    let own_item = find(&e, root, "own");
    set(&e, own_item, "visible", Value::Bool(false));
    frame(&mut s, &e);
    set(&e, own_item, "visible", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(painted(&s, own_item).unwrap().id(), own.id());
    assert_eq!(store.decodes(), 2);
}

#[test]
fn the_budget_drops_the_least_recently_used_picture() {
    let (e, store) = pictures();
    // Room for red (3200 bytes) and blue (9600), not for green (400) too.
    store.set_budget(3200 + 9600 + 100);
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "green"; source: "green.png" }
    Image { objectName: "again"; source: "red.png" }
    Image { objectName: "blue"; source: "blue.png" }
    Image { objectName: "red"; source: "red.png" }
}
"#,
    );
    // Later siblings complete first (as in Qt): red, blue, red, green.
    // Red was used again after blue, so blue is the oldest and goes.
    assert_eq!(
        store.cached_urls(),
        ["file:///red.png", "file:///green.png"]
    );
    assert!(store.bytes() <= store.budget());
    assert_eq!(store.decodes(), 3);
    // Nothing shows through: the items are as they were.
    let blue = find(&e, root, "blue");
    assert_eq!(status(&e, blue), image_status::READY);
    assert_eq!(implicit(&e, blue), (60.0, 40.0));
    assert_eq!(source_size(&e, blue).0, 60.0);
}

#[test]
fn a_picture_on_screen_is_never_dropped() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "seen"; source: "red.png" }
    Image { objectName: "unseen"; source: "blue.png"; visible: false }
}
"#,
    );
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(store.len(), 2);
    store.set_budget(0);
    assert_eq!(
        store.cached_urls(),
        ["file:///red.png"],
        "only the hidden one went"
    );
    assert_eq!(store.trim(MemoryPressure::Critical), 0);
    assert!(painted(&s, find(&e, root, "seen")).is_some());
}

#[test]
fn a_dropped_picture_is_decoded_again_when_its_image_is_shown() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; source: "blue.png"; visible: false }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    assert_eq!((store.decodes(), store.len()), (1, 1));

    // Memory pressure, through the trait the host's memory watcher sees.
    let mut evictable: &ImageStore = &store;
    assert_eq!(evictable.evict(MemoryPressure::Critical), 60 * 40 * 4);
    assert_eq!((store.len(), store.bytes()), (0, 0));
    // The item does not notice.
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(implicit(&e, picture), (60.0, 40.0));
    assert_eq!(num(&e, picture, "progress"), 1.0);
    assert!(painted(&s, picture).is_none());

    set(&e, picture, "visible", Value::Bool(true));
    frame(&mut s, &e);
    let image = painted(&s, picture).expect("painted again");
    assert_eq!((image.width(), image.height()), (60, 40));
    assert_eq!(image.pixel(3, 3), BLUE);
    assert_eq!(store.decodes(), 2, "decoded a second time");
    assert_eq!(status(&e, picture), image_status::READY);
}

#[test]
fn a_picture_that_leaves_the_screen_can_be_dropped_and_comes_back() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; source: "red.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(store.trim(MemoryPressure::Critical), 0, "it is on screen");
    // Scrolled out of the viewport: the scene lets go of the pixels.
    set(&e, picture, "y", Value::Real(500.0));
    frame(&mut s, &e);
    assert!(painted(&s, picture).is_none());
    assert_eq!(store.trim(MemoryPressure::Critical), 40 * 20 * 4);
    set(&e, picture, "y", Value::Real(0.0));
    frame(&mut s, &e);
    assert!(painted(&s, picture).is_some());
    assert_eq!(store.decodes(), 2);
}

#[test]
fn a_dropped_picture_whose_file_went_is_reported_once_and_not_retried() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; source: "red.png"; visible: false }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    store.trim(MemoryPressure::Critical);
    store.remove_resource("file:///red.png");
    set(&e, picture, "visible", Value::Bool(true));
    frame(&mut s, &e);
    assert!(painted(&s, picture).is_none());
    assert_eq!(
        warnings(&e),
        ["a picture dropped to save memory did not load again: Cannot open: file:///red.png"]
    );
    // Another reason to paint does not ask the disk again.
    set(&e, picture, "mirror", Value::Bool(true));
    frame(&mut s, &e);
    assert!(warnings(&e).is_empty());
    assert_eq!(status(&e, picture), image_status::READY);
}

// -------------------------------------------------------------- background

const ASYNC: &str = r#"
import QtQuick
Item {
    id: root
    width: 200; height: 100
    property int changes: 0
    Image {
        objectName: "picture"; asynchronous: true; source: "red.png"
        onStatusChanged: root.changes += 1
    }
}
"#;

#[test]
fn an_asynchronous_image_loads_off_the_ui_thread() {
    let (e, store) = pictures();
    let (tx, rx) = mpsc::channel::<ThreadId>();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&wakes);
    store.set_waker(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(std::thread::current().id());
    });
    let root = load(&e, ASYNC);
    let picture = find(&e, root, "picture");
    // Creation returned without the picture.
    assert_eq!(status(&e, picture), image_status::LOADING);
    assert_eq!(num(&e, picture, "progress"), 0.0);
    assert_eq!(implicit(&e, picture), (0.0, 0.0));
    assert_eq!(num(&e, root, "changes"), 1.0);
    assert_eq!((store.pending(), store.decodes()), (1, 0));

    // The result wakes the host, from the thread that decoded.
    let woke_on = rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("a wake");
    assert_ne!(woke_on, std::thread::current().id());
    assert!(store.has_results());
    // Waking is all that happened: the item changes on the UI thread.
    assert_eq!(status(&e, picture), image_status::LOADING);

    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(num(&e, picture, "progress"), 1.0);
    assert_eq!(implicit(&e, picture), (40.0, 20.0));
    assert_eq!(num(&e, root, "changes"), 2.0);
    assert_eq!(painted(&s, picture).unwrap().pixel(0, 0), RED);
    assert_eq!((store.pending(), store.decodes()), (0, 1));
    assert_eq!(wakes.load(Ordering::SeqCst), 1, "one wake for one result");
    assert!(!s.needs_frame());
}

#[test]
fn a_waiting_result_asks_the_scene_for_a_frame() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; asynchronous: true }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    assert!(!s.needs_frame());
    let (tx, rx) = mpsc::channel::<()>();
    store.set_waker(move || {
        let _ = tx.send(());
    });
    // Held while the decode runs: on a slow machine it can be back before
    // the frame below, which would then deliver it and leave nothing waiting.
    store.set_held(true);
    set(&e, picture, "source", Value::Url("blue.png".into()));
    frame(&mut s, &e);
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .expect("a wake");
    assert_eq!(status(&e, picture), image_status::LOADING);
    store.set_held(false);
    assert!(s.needs_frame(), "the host's repaint finds work");
    frame(&mut s, &e);
    assert_eq!(status(&e, picture), image_status::READY);
    assert!(painted(&s, picture).is_some());
}

#[test]
fn a_held_store_keeps_images_loading_until_finish() {
    let (e, store) = pictures();
    store.set_held(true);
    let root = load(&e, ASYNC);
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    // However long the decode has had, frames do not deliver it.
    ljos_runtime::ComputePool::global().wait_idle();
    for _ in 0..3 {
        frame(&mut s, &e);
        assert!(!s.needs_frame());
    }
    assert_eq!(status(&e, picture), image_status::LOADING);
    store.finish(&e);
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(store.pending(), 0);
}

#[test]
fn a_store_that_starts_after_the_frame_loads_once_the_host_has_drawn() {
    let (e, store) = pictures();
    store.set_start_after_frame(true);
    let woken = std::rc::Rc::new(std::cell::Cell::new(0));
    let w = std::rc::Rc::clone(&woken);
    e.set_timer_wake(move || w.set(w.get() + 1));
    let (tx, rx) = mpsc::channel::<()>();
    store.set_waker(move || {
        let _ = tx.send(());
    });
    let root = load(&e, ASYNC);
    let picture = find(&e, root, "picture");
    // Asked for, the load waits and asks the host for a frame to draw.
    assert_eq!(store.pending(), 1);
    assert!(woken.get() > 0, "the host is asked for a frame");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    ljos_runtime::ComputePool::global().wait_idle();
    assert!(
        !store.has_results(),
        "nothing started before the frame was drawn"
    );
    assert_eq!(status(&e, picture), image_status::LOADING);
    // Drawn, it starts, and its result asks for the next frame.
    store.frame_drawn(&e);
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .expect("a wake");
    frame(&mut s, &e);
    assert_eq!(status(&e, picture), image_status::READY);
    assert!(painted(&s, picture).is_some());
}

#[test]
fn a_store_that_starts_after_the_frame_still_finishes() {
    let (e, store) = pictures();
    store.set_start_after_frame(true);
    let root = load(&e, ASYNC);
    store.finish(&e);
    assert_eq!(status(&e, find(&e, root, "picture")), image_status::READY);
}

#[test]
fn asynchronous_images_share_one_decode_and_the_cache() {
    let (e, store) = pictures();
    store.set_held(true);
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image { objectName: "a"; asynchronous: true; source: "red.png" }
    Image { objectName: "b"; asynchronous: true; source: "red.png" }
    Loader { id: late; objectName: "late"; active: false; sourceComponent: Component {
        Image { objectName: "c"; asynchronous: true; source: "red.png" }
    } }
}
"#,
    );
    assert_eq!(store.pending(), 1, "one job for two waiters");
    store.finish(&e);
    assert_eq!(store.decodes(), 1);
    for name in ["a", "b"] {
        assert_eq!(status(&e, find(&e, root, name)), image_status::READY);
    }
    // A picture that is cached is there at once, asynchronous or not.
    set(&e, find(&e, root, "late"), "active", Value::Bool(true));
    let c = find(&e, root, "c");
    assert_eq!(status(&e, c), image_status::READY);
    assert_eq!(implicit(&e, c), (40.0, 20.0));
    assert_eq!(store.decodes(), 1);
}

#[test]
fn the_old_size_stays_while_the_next_picture_loads() {
    let (e, store) = pictures();
    store.set_held(true);
    let root = load(&e, ASYNC);
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    store.finish(&e);
    frame(&mut s, &e);
    assert!(painted(&s, picture).is_some());

    set(&e, picture, "source", Value::Url("blue.png".into()));
    frame(&mut s, &e);
    assert_eq!(status(&e, picture), image_status::LOADING);
    assert_eq!(num(&e, picture, "progress"), 0.0);
    assert_eq!(implicit(&e, picture), (40.0, 20.0));
    assert_eq!(num(&e, picture, "paintedWidth"), 40.0);
    assert!(painted(&s, picture).is_none(), "the old picture is gone");

    store.finish(&e);
    frame(&mut s, &e);
    assert_eq!(implicit(&e, picture), (60.0, 40.0));
    assert_eq!(painted(&s, picture).unwrap().pixel(0, 0), BLUE);
}

#[test]
fn a_result_for_a_source_left_behind_is_not_shown() {
    let (e, store) = pictures();
    store.set_held(true);
    let root = load(&e, ASYNC);
    let picture = find(&e, root, "picture");
    // Changed, cleared and destroyed while decodes are under way.
    set(&e, picture, "source", Value::Url("blue.png".into()));
    store.finish(&e);
    assert_eq!(implicit(&e, picture), (60.0, 40.0));
    // Red was decoded for nobody, and is cached all the same. The two
    // decodes race on their own threads, so the order they were cached in
    // is not part of what is tested.
    let mut cached = store.cached_urls();
    cached.sort();
    assert_eq!(cached, ["file:///blue.png", "file:///red.png"]);

    set(&e, picture, "source", Value::Url("green.png".into()));
    set(&e, picture, "source", Value::Url("".into()));
    assert_eq!(status(&e, picture), image_status::NULL);
    store.finish(&e);
    assert_eq!(status(&e, picture), image_status::NULL);
    assert_eq!(implicit(&e, picture), (0.0, 0.0));

    set(
        &e,
        picture,
        "source",
        Value::Url("file:///nonexistent/ljos/x.png".into()),
    );
    e.destroy(picture);
    e.process_deferred_deletes();
    store.finish(&e);
    assert!(warnings(&e).is_empty(), "nobody was left to warn");
}

#[test]
fn a_queued_picture_nobody_waits_for_any_more_is_not_decoded() {
    let (e, store) = pictures();
    store.set_held(true);
    // More pictures than run at once: the last ones wait in the queue.
    let names: Vec<String> = (0..12).map(|i| format!("p{i}")).collect();
    for name in &names {
        store.register_resource(format!("file:///{name}.png"), png(4, 4, RED));
    }
    let images: String = names
        .iter()
        .map(|n| {
            format!("Image {{ objectName: \"{n}\"; asynchronous: true; source: \"{n}.png\" }}\n")
        })
        .collect();
    let root = load(&e, &format!("import QtQuick\nItem {{\n{images}}}\n"));
    assert_eq!(store.pending(), 12);
    // All leave before their pictures come, as delegates scrolled away do:
    // the decodes under way run on, those still queued never start.
    for name in &names {
        e.destroy(find(&e, root, name));
    }
    e.process_deferred_deletes();
    store.finish(&e);
    assert_eq!(store.decodes(), 8);
    assert_eq!(store.pending(), 0);
}

#[test]
fn an_asynchronous_failure_is_an_error_when_it_arrives() {
    let (e, store) = pictures();
    store.set_held(true);
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    Image {
        objectName: "picture"; asynchronous: true
        source: "file:///nonexistent/ljos/missing.png"
    }
}
"#,
    );
    let picture = find(&e, root, "picture");
    assert_eq!(status(&e, picture), image_status::LOADING);
    assert!(warnings(&e).is_empty());
    store.finish(&e);
    assert_eq!(status(&e, picture), image_status::ERROR);
    assert_eq!(num(&e, picture, "progress"), 0.0);
    assert_eq!(
        warnings(&e),
        ["file:///Main.qml:4:5: QML Image: Cannot open: file:///nonexistent/ljos/missing.png"]
    );
}

#[test]
fn an_asynchronous_image_reloads_dropped_pixels_in_the_background() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; asynchronous: true; source: "red.png"; visible: false }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    store.finish(&e);
    frame(&mut s, &e);
    store.trim(MemoryPressure::Critical);
    assert!(store.is_empty());

    store.set_held(true);
    set(&e, picture, "visible", Value::Bool(true));
    frame(&mut s, &e);
    // Nothing to paint yet, and the item does not go back to "Loading".
    assert!(painted(&s, picture).is_none());
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(store.pending(), 1);
    // Painting again for another reason does not ask twice.
    set(&e, picture, "mirror", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(store.pending(), 1);

    store.finish(&e);
    frame(&mut s, &e);
    assert_eq!(painted(&s, picture).unwrap().pixel(0, 0), RED);
    assert_eq!(store.decodes(), 2);
}

#[test]
fn more_requests_than_decodes_at_once_all_arrive() {
    let e = engine();
    let store = ImageStore::of(&e);
    let mut items = String::new();
    for i in 0..40u8 {
        store.register_resource(format!("file:///p{i}.png"), png(4, 4, [i, 0, 0, 255]));
        items.push_str(&format!(
            "    Image {{ objectName: \"p{i}\"; asynchronous: true; source: \"p{i}.png\" }}\n"
        ));
    }
    let root = load(&e, &format!("import QtQuick\nItem {{\n{items}}}\n"));
    assert_eq!(store.pending(), 40);
    store.finish(&e);
    assert_eq!((store.pending(), store.decodes(), store.len()), (0, 40, 40));
    for i in 0..40 {
        let item = find(&e, root, &format!("p{i}"));
        assert_eq!(status(&e, item), image_status::READY, "p{i}");
    }
}

// ---------------------------------------------------------------- painting

#[test]
fn the_fill_mode_decides_the_painted_size_and_the_draw_op() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 300; height: 300
    Image { objectName: "picture"; source: "red.png"; width: 100; height: 100 }
    Image { objectName: "empty"; width: 30; height: 20 }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 300.0, 300.0);
    let mut look = |mode: i32| {
        set(&e, picture, "fillMode", Value::Int(mode));
        frame(&mut s, &e);
        let painted = (
            num(&e, picture, "paintedWidth"),
            num(&e, picture, "paintedHeight"),
        );
        let op = ops(&s, picture).into_iter().next().expect("an op");
        let DrawOp::Image { src, dst, tile, .. } = op else {
            panic!("{op:?}")
        };
        (painted, src, dst, tile)
    };
    let r = kurbo::Rect::new;
    let whole = r(0.0, 0.0, 40.0, 20.0);
    let item = r(0.0, 0.0, 100.0, 100.0);
    // Stretch
    assert_eq!(look(0), ((100.0, 100.0), whole, item, None));
    // PreserveAspectFit: centred, 100 by 50.
    assert_eq!(
        look(1),
        ((100.0, 50.0), whole, r(0.0, 25.0, 100.0, 75.0), None)
    );
    // PreserveAspectCrop: 200 by 100, the middle half of the picture shows.
    assert_eq!(
        look(2),
        ((200.0, 100.0), r(10.0, 0.0, 30.0, 20.0), item, None)
    );
    // Tile: copies of the picture's own size, the centre one centred.
    assert_eq!(
        look(3),
        ((100.0, 100.0), whole, item, Some(r(30.0, 40.0, 70.0, 60.0)))
    );
    // Pad: the picture's own size.
    assert_eq!(
        look(6),
        ((40.0, 20.0), whole, r(30.0, 40.0, 70.0, 60.0), None)
    );

    // No picture: the stretching and tiling modes report the item's size.
    let empty = find(&e, root, "empty");
    assert_eq!(num(&e, empty, "paintedWidth"), 30.0);
    set(&e, empty, "fillMode", Value::Int(1));
    assert_eq!(num(&e, empty, "paintedWidth"), 0.0);
    assert!(ops(&s, empty).is_empty());
}

#[test]
fn alignment_mirroring_and_smoothing_reach_the_draw_op() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 300; height: 300
    Image {
        objectName: "picture"; source: "red.png"; width: 100; height: 100
        fillMode: Image.Pad
        horizontalAlignment: Image.AlignRight; verticalAlignment: Image.AlignTop
        mirror: true; mirrorVertically: true; smooth: false; mipmap: true
    }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let op = ops(&s, picture).into_iter().next().expect("an op");
    let DrawOp::Image {
        dst,
        mirror,
        smooth,
        ..
    } = op
    else {
        panic!("{op:?}")
    };
    assert_eq!(dst, kurbo::Rect::new(60.0, 0.0, 100.0, 20.0));
    assert_eq!(mirror, [true, true]);
    assert!(!smooth);
}

#[test]
fn a_border_image_paints_a_nine_patch_with_its_borders() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 300; height: 300
    BorderImage {
        objectName: "frame"; source: "blue.png"; width: 200; height: 100
        border { left: 5; top: 6; right: 7; bottom: 8 }
        horizontalTileMode: BorderImage.Repeat; verticalTileMode: BorderImage.Round
        mirror: true
    }
    BorderImage { objectName: "plain"; source: "red.png" }
}
"#,
    );
    let frame_item = find(&e, root, "frame");
    assert_eq!(implicit(&e, frame_item), (60.0, 40.0));
    assert_eq!(source_size(&e, frame_item).0, 60.0);
    assert_eq!(rect(&e, find(&e, root, "plain")), (0.0, 0.0, 40.0, 20.0));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let op = ops(&s, frame_item).into_iter().next().expect("an op");
    let DrawOp::NinePatch {
        insets,
        dst,
        tile,
        mirror,
        smooth,
        ..
    } = op
    else {
        panic!("{op:?}")
    };
    assert_eq!(
        insets,
        Insets {
            left: 5.0,
            top: 6.0,
            right: 7.0,
            bottom: 8.0
        }
    );
    assert_eq!(dst, kurbo::Rect::new(0.0, 0.0, 200.0, 100.0));
    assert_eq!(tile, [TileMode::Repeat, TileMode::Round]);
    assert!(mirror && smooth);
    // A border changed later paints again.
    set(&e, frame_item, "border.left", Value::Int(9));
    frame(&mut s, &e);
    let op = ops(&s, frame_item).into_iter().next().expect("an op");
    assert!(matches!(op, DrawOp::NinePatch { insets, .. } if insets.left == 9.0));
}

#[test]
fn the_enums_have_qts_values() {
    let (e, _store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    property var fills: [Image.Stretch, Image.PreserveAspectFit, Image.PreserveAspectCrop,
                         Image.Tile, Image.TileVertically, Image.TileHorizontally, Image.Pad]
    property var aligns: [Image.AlignLeft, Image.AlignRight, Image.AlignHCenter,
                          Image.AlignTop, Image.AlignBottom, Image.AlignVCenter]
    property var states: [Image.Null, Image.Ready, Image.Loading, Image.Error,
                          BorderImage.Null, BorderImage.Error]
    property var tiles: [BorderImage.Stretch, BorderImage.Repeat, BorderImage.Round]
    property string all: fills.join() + "|" + aligns.join() + "|" + states.join() + "|" + tiles.join()
}
"#,
    );
    assert_eq!(
        get(&e, root, "all").to_js_string().as_str(),
        "0,1,2,3,4,5,6|1,2,4,32,64,128|0,1,2,3,0,3|0,1,2"
    );
}

// ---------------------------------------------------------------- drawings

/// A 40×20 drawing: blue on the left half, red on the right.
const WIDE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
  <rect width="40" height="20" fill="#ff0000"/>
  <rect width="20" height="20" fill="#0000ff"/>
</svg>"##;

/// The `src` and `dst` of the first image op of `item`.
fn placed(s: &QuickScene, item: ObjectId) -> (kurbo::Rect, kurbo::Rect) {
    ops(s, item)
        .into_iter()
        .find_map(|op| match op {
            DrawOp::Image { src, dst, .. } => Some((src, dst)),
            _ => None,
        })
        .expect("an image op")
}

#[test]
fn a_drawing_on_disk_is_found_without_its_suffix() {
    let dir = std::env::temp_dir().join(format!("ljos-svg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("logo.svg"), WIDE_SVG).unwrap();
    std::fs::write(dir.join("logo.SVGZ"), "not looked at").unwrap();
    // A directory is no picture, whatever its name.
    std::fs::create_dir_all(dir.join("logo.png")).unwrap();
    let e = engine();
    let logo = make::<ljos_quick::Image>(&e, None);
    let path = dir.join("logo").to_string_lossy().into_owned();
    set(&e, logo, "source", Value::Url(path.as_str().into()));
    assert_eq!(status(&e, logo), image_status::READY);
    assert_eq!(implicit(&e, logo), (40.0, 20.0));
    let missing = make::<ljos_quick::Image>(&e, None);
    let gone = dir.join("missing").to_string_lossy().into_owned();
    set(&e, missing, "source", Value::Url(gone.as_str().into()));
    assert_eq!(status(&e, missing), image_status::ERROR);
    assert_eq!(
        warnings(&e)
            .last()
            .map(|w| w.ends_with(&format!("Cannot open: {gone}"))),
        Some(true)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_drawing_is_made_at_the_display_scale_and_placed_in_logical_pixels() {
    let e = engine();
    let store = ImageStore::of(&e);
    store.register_resource("file:///wide.svg", WIDE_SVG.as_bytes());
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; source: "wide.svg" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    let image = painted(&s, picture).unwrap();
    assert_eq!((image.width(), image.height()), (40, 20));

    s.set_scale_factor(2.0);
    frame(&mut s, &e);
    let sharp = painted(&s, picture).unwrap();
    assert_eq!((sharp.width(), sharp.height()), (80, 40));
    assert_eq!(sharp.pixel(39, 20), BLUE);
    assert_eq!(sharp.pixel(40, 20), RED);
    // The item still says what it did, and paints over the same area.
    assert_eq!(implicit(&e, picture), (40.0, 20.0));
    assert_eq!(source_size(&e, picture), (40.0, 20.0));
    let (src, dst) = placed(&s, picture);
    assert_eq!(src, kurbo::Rect::new(0.0, 0.0, 80.0, 40.0));
    assert_eq!(dst, kurbo::Rect::new(0.0, 0.0, 40.0, 20.0));

    // Painting again for another reason draws nothing anew.
    let decodes = store.decodes();
    set(&e, picture, "mirror", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(painted(&s, picture).unwrap().id(), sharp.id());
    assert_eq!(store.decodes(), decodes);
    // Back at one, the first drawing is still in the cache.
    s.set_scale_factor(1.0);
    frame(&mut s, &e);
    assert_eq!(painted(&s, picture).unwrap().id(), image.id());
    assert_eq!(store.decodes(), decodes);
}

#[test]
fn a_picture_is_not_made_again_at_the_display_scale() {
    let (e, store) = pictures();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; source: "red.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    s.set_scale_factor(2.0);
    frame(&mut s, &e);
    let image = painted(&s, picture).unwrap();
    assert_eq!((image.width(), image.height()), (40, 20));
    assert_eq!((store.decodes(), store.len()), (1, 1));
}

#[test]
fn an_asynchronous_drawing_is_made_sharper_in_the_background() {
    let e = engine();
    let store = ImageStore::of(&e);
    store.register_resource("file:///wide.svg", WIDE_SVG.as_bytes());
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; asynchronous: true; source: "wide.svg" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    store.finish(&e);
    frame(&mut s, &e);
    store.set_held(true);
    s.set_scale_factor(2.0);
    frame(&mut s, &e);
    // Meanwhile the drawing at one device pixel per logical pixel.
    assert_eq!(painted(&s, picture).unwrap().width(), 40);
    assert_eq!(store.pending(), 1);
    set(&e, picture, "mirror", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(store.pending(), 1, "asked once");
    store.finish(&e);
    frame(&mut s, &e);
    assert_eq!(painted(&s, picture).unwrap().width(), 80);
    assert_eq!(
        placed(&s, picture).1,
        kurbo::Rect::new(0.0, 0.0, 40.0, 20.0)
    );
}

#[test]
fn a_drawing_that_does_not_cache_keeps_its_sharp_pixels() {
    let e = engine();
    let store = ImageStore::of(&e);
    store.register_resource("file:///wide.svg", WIDE_SVG.as_bytes());
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; cache: false; source: "wide.svg" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    s.set_scale_factor(1.5);
    frame(&mut s, &e);
    let sharp = painted(&s, picture).unwrap();
    assert_eq!((sharp.width(), sharp.height()), (60, 30));
    let decodes = store.decodes();
    set(&e, picture, "mirror", Value::Bool(true));
    frame(&mut s, &e);
    assert_eq!(painted(&s, picture).unwrap().id(), sharp.id());
    assert_eq!(store.decodes(), decodes);
}

// ------------------------------------------------- pictures larger than the view

#[test]
fn a_picture_larger_than_the_view_keeps_fewer_pixels_and_its_own_size() {
    let (e, store) = pictures();
    store.register_resource("file:///big.png", png(2048, 1024, BLUE));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; anchors.fill: parent }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    set(&e, picture, "source", Value::Url("file:///big.png".into()));
    frame(&mut s, &e);
    assert_eq!(status(&e, picture), image_status::READY);
    // What QML sees is the picture's own size.
    assert_eq!(implicit(&e, picture), (2048.0, 1024.0));
    assert_eq!(source_size(&e, picture), (2048.0, 1024.0));
    // Its pixels are decoded on another thread.
    assert!(painted(&s, picture).is_none());
    store.finish(&e);
    frame(&mut s, &e);
    // What is kept is enough for the view, 200 long rounded up to a
    // quarter octave.
    let image = painted(&s, picture).unwrap();
    assert_eq!((image.width(), image.height()), (224, 112));
    assert_eq!(image.pixel(10, 10), BLUE);
    assert_eq!(store.bytes(), 224 * 112 * 4);
}

#[test]
fn a_large_picture_whose_body_is_broken_fails_once_it_is_decoded() {
    let (e, store) = pictures();
    // A whole header, then rows cut short.
    let mut bytes = png(1024, 768, BLUE);
    bytes.truncate(bytes.len() / 2);
    store.register_resource("file:///cut.png", bytes);
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; width: 200; height: 100; source: "cut.png" }
}
"#,
    );
    let picture = find(&e, root, "picture");
    // Its header is all that is read at first.
    assert_eq!(status(&e, picture), image_status::READY);
    assert_eq!(implicit(&e, picture), (1024.0, 768.0));
    store.finish(&e);
    assert_eq!(status(&e, picture), image_status::ERROR);
    assert_eq!(
        warnings(&e),
        [
            "file:///Main.qml:5:5: QML Image: Error decoding: file:///cut.png: unexpected end of file"
        ]
    );
}

#[test]
fn a_reduced_picture_drawn_larger_is_decoded_again_sharper() {
    let (e, store) = pictures();
    store.register_resource("file:///big.png", png(2048, 1024, BLUE));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; width: 200; height: 100 }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    set(&e, picture, "source", Value::Url("file:///big.png".into()));
    store.finish(&e);
    frame(&mut s, &e);
    assert_eq!(painted(&s, picture).unwrap().width(), 224);
    // Four times as wide on screen needs four times the pixels.
    set(&e, picture, "scale", Value::from(4.0));
    frame(&mut s, &e);
    // A picture decoded aside is sharpened aside too: wait for it rather
    // than for a time a slow machine may not keep to.
    store.finish(&e);
    frame(&mut s, &e);
    let image = painted(&s, picture).unwrap();
    assert_eq!((image.width(), image.height()), (896, 448));
    assert_eq!(implicit(&e, picture), (2048.0, 1024.0));
}

#[test]
fn a_picture_that_grows_into_view_by_its_scale_is_decoded_once() {
    let (e, store) = pictures();
    store.register_resource("file:///big.png", png(2048, 1024, BLUE));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 200; height: 100
    Image { objectName: "picture"; width: 200; height: 100; scale: 0 }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 200.0, 100.0);
    frame(&mut s, &e);
    set(&e, picture, "source", Value::Url("file:///big.png".into()));
    // A pop-in: the scale goes from nothing to 1 over a few frames.
    for step in 0..=20 {
        set(&e, picture, "scale", Value::from(f64::from(step) / 20.0));
        frame(&mut s, &e);
    }
    // A picture this large is decoded aside.
    store.finish(&e);
    frame(&mut s, &e);
    let image = painted(&s, picture).unwrap();
    assert_eq!((image.width(), image.height()), (224, 112));
    // Kept for the item's own size from the start: the frames between
    // drew it smaller and decoded nothing again.
    assert_eq!(store.decodes(), 1);
}

#[test]
fn a_picture_drawn_much_smaller_than_it_is_kept_is_kept_smaller() {
    let (e, store) = pictures();
    store.register_resource("file:///big.png", png(500, 250, BLUE));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Image { objectName: "picture"; source: "big.png"; width: 100; height: 50 }
    Image { objectName: "other"; width: 400; height: 200 }
}
"#,
    );
    let picture = find(&e, root, "picture");
    // Small enough to be decoded on the spot, before any scene says how
    // large it is drawn: kept whole.
    assert_eq!(store.bytes(), 500 * 250 * 4);
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let image = painted(&s, picture).unwrap();
    assert_eq!((image.width(), image.height()), (112, 56));
    assert_eq!(image.pixel(5, 5), BLUE);
    assert_eq!(implicit(&e, picture), (500.0, 250.0));
    assert_eq!(source_size(&e, picture), (500.0, 250.0));
    // The whole picture is no longer kept, and nothing was read again.
    assert_eq!(store.bytes(), 112 * 56 * 4);
    assert_eq!(store.decodes(), 1);

    // Another Image of it starts from what is kept, and is made sharper
    // for the size it is drawn at.
    let other = find(&e, root, "other");
    set(&e, other, "source", Value::Url("big.png".into()));
    assert_eq!(status(&e, other), image_status::READY);
    assert_eq!(implicit(&e, other), (500.0, 250.0));
    frame(&mut s, &e);
    let image = painted(&s, other).unwrap();
    assert_eq!((image.width(), image.height()), (448, 224));
    assert_eq!(store.decodes(), 2);
}

/// A fetcher that counts what it is asked for.
struct Counting {
    bytes: Vec<u8>,
    asked: Arc<AtomicUsize>,
}

impl ljos_platform::Fetcher for Counting {
    fn fetch(&self, _url: &str) -> Result<Vec<u8>, ljos_platform::FetchError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(self.bytes.clone())
    }
}

#[test]
fn a_picture_of_an_item_not_painted_is_kept_at_the_items_size() {
    let (e, store) = pictures();
    store.register_resource("file:///big.png", png(2048, 1024, BLUE));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Image { objectName: "picture"; visible: false; width: 100; height: 50 }
}
"#,
    );
    let picture = find(&e, root, "picture");
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    set(&e, picture, "source", Value::Url("file:///big.png".into()));
    frame(&mut s, &e);
    store.finish(&e);
    // Never painted, it keeps what its size needs, not the view's 448.
    assert_eq!(store.bytes(), 112 * 56 * 4);
    assert_eq!(implicit(&e, picture), (2048.0, 1024.0));
    assert_eq!(source_size(&e, picture), (2048.0, 1024.0));
    // Shown, it paints those pixels, with nothing read again.
    set(&e, picture, "visible", Value::from(true));
    frame(&mut s, &e);
    let image = painted(&s, picture).unwrap();
    assert_eq!((image.width(), image.height()), (112, 56));
    assert_eq!(image.pixel(5, 5), BLUE);
    assert_eq!(store.decodes(), 1);
}

#[test]
fn a_picture_not_painted_follows_its_fill_mode_source_size_and_scale() {
    let (e, store) = pictures();
    store.register_resource("file:///big.png", png(2048, 1024, BLUE));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Image {
        objectName: "fit"; visible: false; width: 100; height: 400
        fillMode: Image.PreserveAspectFit
    }
    Image {
        objectName: "asked"; visible: false; width: 100; height: 50
        sourceSize.width: 512
    }
    Image {
        objectName: "tiled"; visible: false; width: 100; height: 50
        fillMode: Image.Tile
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let mut bytes = |name: &str| {
        let item = find(&e, root, name);
        let before = store.bytes();
        set(&e, item, "source", Value::Url("file:///big.png".into()));
        frame(&mut s, &e);
        store.finish(&e);
        let kept = store.bytes() - before;
        set(&e, item, "source", Value::from(""));
        store.trim(MemoryPressure::Critical);
        kept
    };
    // Fitted into 100 by 400 it is drawn 100 by 50.
    assert_eq!(bytes("fit"), 112 * 56 * 4);
    // Asked for at 512 by 256 and drawn at 100 by 50; it still says 512.
    assert_eq!(bytes("asked"), 112 * 56 * 4);
    assert_eq!(source_size(&e, find(&e, root, "asked")), (512.0, 0.0));
    // A tile is drawn at the picture's size: as much as the view shows.
    assert_eq!(bytes("tiled"), 448 * 224 * 4);

    // On a display of two device pixels to the logical pixel, twice as
    // many.
    store.show_at(&e, 7, 2.0, 400.0);
    assert_eq!(bytes("fit"), 224 * 112 * 4);
}

#[test]
fn a_network_picture_of_a_row_not_painted_is_kept_small_and_not_fetched_again() {
    let (e, store) = pictures();
    let asked = Arc::new(AtomicUsize::new(0));
    ljos_platform::Platform::of(&e).set_fetcher(Arc::new(Counting {
        bytes: png(2048, 1024, GREEN),
        asked: asked.clone(),
    }));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Image {
        objectName: "row"; visible: false
        width: Math.min(200, sourceSize.width)
        fillMode: Image.PreserveAspectFit
    }
}
"#,
    );
    let row = find(&e, root, "row");
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    set(
        &e,
        row,
        "source",
        Value::from("http://example.invalid/a.png"),
    );
    store.finish(&e);
    frame(&mut s, &e);
    assert_eq!(status(&e, row), image_status::READY);
    // 200 wide, its height the picture's: drawn 200 by 100.
    assert_eq!(store.bytes(), 224 * 112 * 4);
    set(&e, row, "visible", Value::from(true));
    frame(&mut s, &e);
    store.finish(&e);
    frame(&mut s, &e);
    let image = painted(&s, row).unwrap();
    assert_eq!((image.width(), image.height()), (224, 112));
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}

#[test]
fn a_picture_not_painted_is_kept_small_once_its_item_is_given_a_size() {
    let (e, store) = pictures();
    store.register_resource("file:///big.png", png(2048, 1024, BLUE));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Item {
        objectName: "row"; visible: false; width: 0
        Image {
            objectName: "picture"; width: parent.width / 2; height: 50
        }
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let picture = find(&e, root, "picture");
    set(&e, picture, "source", Value::Url("file:///big.png".into()));
    frame(&mut s, &e);
    store.finish(&e);
    // With no size it draws nothing yet, and is kept as the view needs.
    assert_eq!(store.bytes(), 448 * 224 * 4);
    // A row laid out later, never painted: kept as drawn, 100 by 50.
    set(&e, find(&e, root, "row"), "width", Value::from(200));
    frame(&mut s, &e);
    assert_eq!(store.bytes(), 112 * 56 * 4);
    assert_eq!(store.decodes(), 1);
}

#[test]
fn a_network_picture_that_arrives_for_a_row_gone_keeps_only_what_it_is_made_from() {
    let (e, store) = pictures();
    let asked = Arc::new(AtomicUsize::new(0));
    ljos_platform::Platform::of(&e).set_fetcher(Arc::new(Counting {
        bytes: png(2048, 1024, GREEN),
        asked: asked.clone(),
    }));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Image { objectName: "row"; width: 100; height: 50 }
    Image { objectName: "later"; width: 100; height: 50 }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let url = Value::from("http://example.invalid/a.png");
    set(&e, find(&e, root, "row"), "source", url.clone());
    // The row is let go while its picture is on its way.
    let row = find(&e, root, "row");
    e.destroy(row);
    frame(&mut s, &e);
    assert!(!e.is_alive(row));
    store.finish(&e);
    assert_eq!(store.bytes(), 448 * 224 * 4);
    assert_eq!(store.resident_bytes(), 0);
    // A row made later has it from what it was read from.
    let later = find(&e, root, "later");
    set(&e, later, "source", url);
    frame(&mut s, &e);
    store.finish(&e);
    frame(&mut s, &e);
    assert_eq!(status(&e, later), image_status::READY);
    let image = painted(&s, later).unwrap();
    assert_eq!((image.width(), image.height()), (112, 56));
    assert_eq!(image.pixel(5, 5), GREEN);
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}

/// Lets the pixels of what `item` paints go, as an upload does, and says
/// whether they went and whether reading them gives them back as they were.
fn let_go_and_read_back(s: &QuickScene, item: ObjectId) -> (bool, bool) {
    let image = painted(s, item).unwrap();
    let before = image.data().to_vec();
    let went = image.let_go();
    let gone = !image.is_resident();
    (went && gone, image.data().to_vec() == before)
}

#[test]
fn a_shown_picture_can_be_let_go_and_made_again_from_what_it_was_read_from() {
    let dir = std::env::temp_dir().join(format!("ljos-again-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("big.png"), png(2048, 1024, BLUE)).unwrap();
    let (e, store) = pictures();
    store.register_resource("file:///small.png", png(64, 32, RED));
    let asked = Arc::new(AtomicUsize::new(0));
    ljos_platform::Platform::of(&e).set_fetcher(Arc::new(Counting {
        bytes: png(512, 256, GREEN),
        asked: asked.clone(),
    }));
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Column {
        Image { objectName: "file"; width: 100; height: 50 }
        Image { objectName: "held"; width: 64; height: 32 }
        Image { objectName: "web"; width: 100; height: 50 }
        Image { objectName: "hidden"; visible: false; width: 50; height: 25 }
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    let file = format!("file://{}", dir.join("big.png").display());
    set(
        &e,
        find(&e, root, "file"),
        "source",
        Value::Url(file.as_str().into()),
    );
    set(
        &e,
        find(&e, root, "held"),
        "source",
        Value::Url("file:///small.png".into()),
    );
    set(
        &e,
        find(&e, root, "web"),
        "source",
        Value::from("http://example.invalid/a.png"),
    );
    set(
        &e,
        find(&e, root, "hidden"),
        "source",
        Value::Url(file.as_str().into()),
    );
    store.finish(&e);
    frame(&mut s, &e);
    store.finish(&e);
    frame(&mut s, &e);
    // Read from a file, shrunk when painted: the file is read again.
    assert_eq!(
        let_go_and_read_back(&s, find(&e, root, "file")),
        (true, true)
    );
    // Registered bytes are held anyway.
    assert_eq!(
        let_go_and_read_back(&s, find(&e, root, "held")),
        (true, true)
    );
    // From the web, from the bytes fetched, which are fewer than the
    // pixels: not fetched again.
    assert_eq!(
        let_go_and_read_back(&s, find(&e, root, "web")),
        (true, true)
    );
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    // Shrunk to its item's size after the load, never painted.
    let hidden = find(&e, root, "hidden");
    set(&e, hidden, "visible", Value::from(true));
    frame(&mut s, &e);
    assert_eq!(let_go_and_read_back(&s, hidden), (true, true));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_picture_whose_bytes_outweigh_its_pixels_is_not_let_go() {
    let (e, store) = pictures();
    // A picture noisy in one quarter, kept small: its bytes are fewer
    // than its pixels and more than what is kept.
    let noise: Vec<u8> = (0..512u32 * 512)
        .flat_map(|i| {
            let n = i.wrapping_mul(2_654_435_761);
            if i % 512 < 256 && i / 512 < 256 {
                [(n >> 8) as u8, (n >> 16) as u8, (n >> 24) as u8, 255]
            } else {
                [32, 96, 160, 255]
            }
        })
        .collect();
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, 512, 512);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&noise)
            .unwrap();
    }
    let encoded = bytes.len();
    assert!(encoded < 512 * 512 * 4 && encoded > 56 * 56 * 4);
    store.register_resource("file:///noise.png", bytes);
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 300
    Image { objectName: "noise"; width: 50; height: 50; source: "file:///noise.png" }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 300.0);
    frame(&mut s, &e);
    store.finish(&e);
    frame(&mut s, &e);
    let image = painted(&s, find(&e, root, "noise")).unwrap();
    assert!(image.byte_len() < 512 * 512 * 4);
    assert!(!image.let_go());
    assert!(image.is_resident());
    // Nor does it hold the bytes it was read from, which it never needs.
    assert!(image.made_again_by().is_none());
}
