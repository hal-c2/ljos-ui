//! Takes pictures of items from Rust, with no script to ask, as a host
//! that keeps thumbnails or checks what a view draws does
//! ([`ljos::item::grab`]). A worker thread asks once the window is up
//! and the application is idle; each ask is a message, and nothing else
//! happens in the application meanwhile. It asks for a picture of
//! `card` and writes it to `DIR/card.png`, then asks for pictures of two
//! items it destroys in the same turn, one as QML's `destroy()` does and
//! one at once: each is called back with no picture. It prints when
//! each ask was made and when its callback came. It fails when a
//! callback does not come within two seconds, when `card` gives no
//! picture or its file cannot be written, and when a destroyed item
//! gives one. It stays idle for two seconds before the first ask and
//! after the last callback, then quits.
//!
//! `cargo run -p ljos --example grab_item -- DIR` — opens a window: run
//! it in the off-screen sandbox (`mise run desktop:cua:exec …`).

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ljos::item::ImageHandle;
use ljos::prelude::*;

/// What the worker asks the Engine thread to do.
enum Ask {
    /// A picture of the item of that `objectName`, told on `reply`. With
    /// `doomed`, the item is destroyed in the same turn: at once (`true`)
    /// or as QML's `destroy()` does, at the next pass (`false`).
    Grab {
        name: &'static str,
        doomed: Option<bool>,
        reply: mpsc::Sender<Heard>,
    },
    Finish,
}

/// What a grab came to, for the worker: the size of the picture, if one
/// came, or why there is none to keep. Each ask has a channel of its
/// own, so a callback that comes late is never taken for the next one's.
type Heard = Result<Option<(u32, u32)>, String>;

fn main() -> ExitCode {
    let Some(out) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: grab_item DIR");
        return ExitCode::from(2);
    };
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("grab_item: {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    let (failed_tx, failed_rx) = mpsc::channel::<String>();
    let start = Instant::now();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/grab_item");
    // The root is made in the setup, which runs after the builder.
    let root: Rc<Cell<Option<ObjectId>>> = Rc::default();
    let at = Rc::clone(&root);
    let app = ljos::App::new("Grab an item")
        .qml_dir(dir, "Main")
        .expect("the example's QML compiles")
        .on_message(move |engine, ask: Ask| answer(engine, at.get(), ask, &out, start));
    let ran = app.run_with(move |setup| {
        let made = setup
            .engine()
            .create_from_url("Main", None, Vec::new())
            .map_err(|e| -> ljos::SetupError { e.to_string().into() })?;
        root.set(Some(made));
        let ui = setup.handle();
        std::thread::spawn(move || {
            // The window is up and its first frames are drawn by then:
            // the application is idle when the first ask comes.
            std::thread::sleep(Duration::from_millis(2000));
            for (name, doomed) in [
                ("card", None),
                ("doomed", Some(false)),
                ("doomedNow", Some(true)),
            ] {
                let pictured = doomed.is_none();
                let (reply, heard) = mpsc::channel::<Heard>();
                if ui
                    .send(Ask::Grab {
                        name,
                        doomed,
                        reply,
                    })
                    .is_err()
                {
                    let _ = failed_tx.send(format!("{name}: the application is gone"));
                    return;
                }
                let failed = match heard.recv_timeout(Duration::from_secs(2)) {
                    Ok(Err(e)) => Some(format!("{name}: {e}")),
                    Ok(Ok(Some((w, h)))) if !pictured => {
                        Some(format!("{name}: a destroyed item gave a {w}x{h} picture"))
                    }
                    Ok(Ok(None)) if pictured => Some(format!("{name}: no picture came")),
                    Ok(_) => None,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        Some(format!("{name}: no callback within 2 s"))
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        Some(format!("{name}: the application is gone"))
                    }
                };
                if let Some(f) = failed {
                    let _ = failed_tx.send(f);
                }
                // Idle between asks, so each is asked of an idle application.
                std::thread::sleep(Duration::from_millis(300));
            }
            // Idle again before it quits, so a watcher outside sees that
            // nothing runs once every callback came.
            std::thread::sleep(Duration::from_millis(2000));
            let _ = ui.send(Ask::Finish);
        });
        Ok(made)
    });
    if let Err(e) = ran {
        eprintln!("grab_item: {e}");
        return ExitCode::FAILURE;
    }
    // The window may close while the worker still waits to ask: reading
    // until the worker drops its sender waits for what it has to report.
    let failed: Vec<String> = failed_rx.iter().collect();
    for f in &failed {
        eprintln!("grab_item: {f}");
    }
    if failed.is_empty() {
        eprintln!("grab_item: every grab was called back");
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Asks for the picture `ask` names, on the Engine thread, of the items
/// under `root`: none before the setup has made it.
fn answer(engine: &Engine, root: Option<ObjectId>, ask: Ask, out: &Path, start: Instant) {
    let (name, doomed, reply) = match ask {
        Ask::Grab {
            name,
            doomed,
            reply,
        } => (name, doomed, reply),
        Ask::Finish => {
            if let Some(root) = root {
                let _ = engine.invoke_by_name(root, "finish", &[]);
            }
            return;
        }
    };
    let Some(root) = root else {
        let _ = reply.send(Err("asked before the window's root was made".to_owned()));
        return;
    };
    let Some(item) = named(engine, root, name) else {
        let _ = reply.send(Err("no item has that objectName".to_owned()));
        return;
    };
    let heard = reply.clone();
    let file = out.join(format!("{name}.png"));
    let asked = ljos::item::grab(engine, item, move |_, picture| {
        let size = picture.as_ref().map(|p| (p.width(), p.height()));
        eprintln!(
            "[{:>6} ms] grab of {name}: called back with {}",
            start.elapsed().as_millis(),
            size.map_or_else(|| "no picture".to_owned(), |(w, h)| format!("{w}x{h}")),
        );
        let kept = match &picture {
            Some(p) => write_png(&file, p).map_err(|e| format!("{}: {e}", file.display())),
            None => Ok(()),
        };
        let _ = heard.send(kept.map(|()| size));
    });
    // A refused ask never calls back, so the worker is told here.
    if let Err(why) = &asked {
        let _ = reply.send(Err(format!("refused: {why}")));
    }
    eprintln!(
        "[{:>6} ms] grab of {name}: asked ({})",
        start.elapsed().as_millis(),
        match (&asked, doomed) {
            (Err(why), _) => format!("refused: {why}"),
            (Ok(()), None) => "kept".to_owned(),
            (Ok(()), Some(false)) => "destroyed at the next pass".to_owned(),
            (Ok(()), Some(true)) => "destroyed at once".to_owned(),
        },
    );
    match doomed {
        Some(true) => {
            engine.destroy_now(item);
        }
        Some(false) => {
            engine.destroy(item);
        }
        None => {}
    }
}

/// The first item named `name` at or below `root`, depth first.
fn named(engine: &Engine, root: ObjectId, name: &str) -> Option<ObjectId> {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if matches!(engine.read_by_name(o, "objectName"), Value::String(s) if s.as_str() == name) {
            return Some(o);
        }
        stack.extend(engine.children(o));
    }
    None
}

/// Writes `picture` as a PNG file, with straight alpha as a file holds
/// it: an `ImageHandle`'s pixels are premultiplied.
fn write_png(path: &Path, picture: &ImageHandle) -> Result<(), Box<dyn std::error::Error>> {
    let mut data = picture.data().to_vec();
    for p in data.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, picture.width(), picture.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&data)?;
    Ok(())
}
