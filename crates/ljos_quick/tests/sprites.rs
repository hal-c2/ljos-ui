//! What a `SpriteSequence` costs the frames it plays in. Its behaviour is
//! in `features/animation/sprite-sequence.feature`.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use common::*;
use ljos_engine::Value;
use ljos_quick::ImageStore;
use ljos_runtime::{Time, VirtualClock};

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&vec![200; (width * height * 4) as usize])
            .unwrap();
    }
    out
}

#[test]
fn a_jump_finds_its_sheet_taken_in_when_the_sequence_started() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let store = ImageStore::of(&e);
    store.register_resource("file:///walk.png", png(64, 16));
    store.register_resource("file:///fall.png", png(64, 16));
    let root = load(
        &e,
        r#"
import QtQuick
SpriteSequence {
    width: 16; height: 16
    Sprite { name: "walk"; source: "file:///walk.png"; frameCount: 4; frameDuration: 100 }
    Sprite { name: "walk2"; source: "file:///walk.png"; frameCount: 4; frameDuration: 100 }
    Sprite { name: "fall"; source: "file:///fall.png"; frameCount: 4; frameDuration: 100 }
}
"#,
    );
    let at = Cell::new(0);
    let pass = |ms: u64| {
        at.set(at.get() + ms);
        clock.set(Time::from_millis(at.get()));
        e.advance_timers(Time::from_millis(at.get()));
    };
    pass(50);
    // Each sheet once, the one not shown yet away from the frame.
    store.finish(&e);
    assert_eq!(
        store.decodes(),
        2,
        "the sheet not shown yet was not taken in"
    );
    assert_eq!(get(&e, root, "currentSprite"), Value::from("walk"));

    e.invoke_by_name(root, "jumpTo", &[Value::from("fall")])
        .unwrap();
    pass(50);
    assert_eq!(get(&e, root, "currentSprite"), Value::from("fall"));
    assert_eq!(store.decodes(), 2, "the jump decoded its sheet again");
    // Each frame is cut from the sheet's pixels, not decoded from its file.
    for _ in 0..8 {
        pass(100);
    }
    assert_eq!(store.decodes(), 2, "a frame decoded its sheet again");
}
