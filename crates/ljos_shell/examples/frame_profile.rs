//! Where a frame's time goes, without a window: a QML project is shown in
//! a `ShellApp` on an `egui::Context` driven at 60 Hz on a virtual clock,
//! and each pass is timed in three parts: the pass (inbox, scene frame,
//! encode into egui's shapes, egui's end of pass), tessellation, and the
//! rest. With `--click` a 5 × 5 grid over the window is clicked meanwhile,
//! as `ljos bench` does. Run it under `valgrind --tool=callgrind` to see
//! the pass split by function.
//!
//! `cargo run --release -p ljos_shell --example frame_profile -- DIR ENTRY
//! [--frames N] [--size WxH] [--click] [--warm N] [--slow MS] [--start EXPR]…
//! [--apart] [--eval EXPR]… [--wall] [--grow PX]`
//!
//! `--start` runs `EXPR` in the root after the warm frames and prints how
//! long it took and how much of the graph it ran, to time one handler (a
//! game's start, a page's build) on its own. `--slow` prints each frame
//! that took longer than `MS`. With `--apart` each `--start` runs before a
//! frame of its own, the way a script reaches a shown window, so it finds
//! what that frame read up to date again; run back to back, a later one
//! finds it still stale from the one before and costs less than it would.
//!
//! `--grow` makes the window `PX` wider and narrower again on alternate
//! frames after the warm ones, to time what a resize costs.
//!
//! `--eval` prints what a script in the root makes of `EXPR` at the end,
//! to see that the run reached the state meant (a game started, a page
//! shown).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use ljos_engine::MapSource;
use ljos_runtime::VirtualClock;
use ljos_shell::{ShellApp, ShellConfig};

/// 2026-01-01 12:00:00 UTC: what `Date` says as the run starts.
const WALL_ORIGIN_MS: f64 = 1_767_268_800_000.0;

struct Args {
    dir: PathBuf,
    entry: String,
    frames: u32,
    warm: u32,
    size: (f32, f32),
    click: bool,
    eval: Vec<String>,
    start: Vec<String>,
    apart: bool,
    slow: Option<Duration>,
    wall: bool,
    grow: f32,
}

fn args() -> anyhow::Result<Args> {
    let mut it = std::env::args().skip(1);
    let dir = PathBuf::from(it.next().context("DIR")?);
    let entry = it.next().unwrap_or_else(|| "Main.qml".into());
    let mut a = Args {
        dir,
        entry,
        frames: 600,
        warm: 60,
        size: (800.0, 600.0),
        click: false,
        eval: Vec::new(),
        start: Vec::new(),
        apart: false,
        slow: None,
        wall: false,
        grow: 0.0,
    };
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--frames" => a.frames = it.next().context("N")?.parse()?,
            "--warm" => a.warm = it.next().context("N")?.parse()?,
            "--click" => a.click = true,
            "--apart" => a.apart = true,
            "--wall" => a.wall = true,
            "--grow" => a.grow = it.next().context("PX")?.parse()?,
            "--slow" => {
                let ms: f64 = it.next().context("MS")?.parse()?;
                a.slow = Some(Duration::from_secs_f64(ms / 1000.0));
            }
            "--eval" => a.eval.push(it.next().context("EXPR")?),
            "--start" => a.start.push(it.next().context("EXPR")?),
            "--size" => {
                let s = it.next().context("WxH")?;
                let (w, h) = s.split_once('x').context("WxH")?;
                a.size = (w.parse()?, h.parse()?);
            }
            other => bail!("unknown argument {other}"),
        }
    }
    Ok(a)
}

fn compile(dir: &std::path::Path) -> anyhow::Result<MapSource> {
    let base = format!("file://{}", dir.to_string_lossy());
    let mut source = MapSource::new().with_base_url(base);
    for r in ljos_compiler::compile_project(dir)? {
        match r.result {
            Ok(c) if !c.has_errors() => source.insert(r.key, c.file),
            _ => eprintln!("{} does not compile", r.key),
        }
    }
    Ok(source)
}

fn main() -> anyhow::Result<()> {
    let a = args()?;
    let dir = a.dir.canonicalize()?;
    let key = a.entry.trim_end_matches(".qml").to_owned();
    let source = compile(&dir)?;
    let ctx = egui::Context::default();
    let clock = Arc::new(VirtualClock::new());
    let config = ShellConfig::new("profile").with_size(a.size.0, a.size.1);
    let root = std::cell::Cell::new(None);
    let mut app = ShellApp::<()>::new(&ctx, &config, clock.clone(), |setup| {
        let engine = setup.engine();
        // Timers and `Date` move with the frames, as they would in real
        // time, so a Timer fires and a clock's second ticks within the run.
        engine.set_clock(std::rc::Rc::new(clock.clone()));
        // `--wall` leaves `Date` on the machine's clock, as an app's is.
        if !a.wall {
            engine.set_wall_clock_origin(Some(WALL_ORIGIN_MS));
        }
        ljos_localstorage::register(engine);
        ljos_localstorage::set_storage(engine, ljos_localstorage::Storage::Memory);
        engine.set_component_source(source);
        let made = engine
            .create_from_url(&key, None, Vec::new())
            .map_err(|e| e.to_string())?;
        root.set(Some(made));
        // Under a profiler a handler runs many times slower: interrupting
        // it would measure a different run. This starts the script host
        // if nothing has, which happens before the frames.
        let _ = engine.set_js_dispatch_budget(Duration::from_secs(10), Duration::from_secs(600));
        Ok(made)
    })?;
    let screen = |i: u32| {
        let wider = i >= a.warm && (i - a.warm) % 2 == 1;
        let w = a.size.0 + if wider { a.grow } else { 0.0 };
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, a.size.1))
    };
    let period = Duration::from_micros(16_667);
    let total = a.warm + a.frames;
    // One click every 24 frames (0.4 s), over the same 5 × 5 grid as the bench.
    let click_at = |i: u32| -> Option<egui::Pos2> {
        if !a.click || i < a.warm || !(i - a.warm).is_multiple_of(24) {
            return None;
        }
        let n = ((i - a.warm) / 24) % 25;
        let (row, col) = (n / 5, n % 5);
        Some(egui::pos2(
            a.size.0 * (2 * col + 1) as f32 / 10.0,
            a.size.1 * (2 * row + 1) as f32 / 10.0,
        ))
    };
    let (mut pass, mut tess, mut prims) = (Duration::ZERO, Duration::ZERO, 0usize);
    let (mut worst, mut worst_at) = (Duration::ZERO, 0);
    // Frames after which the view asked for the next one at once: what a
    // real host would draw; the rest it would skip until a deadline.
    let mut eager = 0u32;
    let mut press: Option<egui::Pos2> = None;
    let mut before = None;
    for i in 0..total {
        let starts: &[String] = match i.checked_sub(a.warm) {
            Some(k) if a.apart => a.start.get(k as usize..=k as usize).unwrap_or_default(),
            Some(0) => &a.start,
            _ => &[],
        };
        if let Some(root) = root.get() {
            for e in starts {
                let t = Instant::now();
                let s0 = app.engine().graph().stats();
                let said = app.engine().eval_js_in(
                    Some(root),
                    e,
                    Duration::from_secs(600),
                    [4, 32, 200, 1000, 4000],
                );
                let took = t.elapsed();
                let s1 = app.engine().graph().stats();
                match said {
                    Ok(v) => println!("{e}: {v} in {took:.2?}"),
                    Err(err) => println!("{e}: {err} in {took:.2?}"),
                }
                println!(
                    "  {} evaluations, {} effect runs, {} propagations, {} flushes",
                    s1.evaluations - s0.evaluations,
                    s1.effect_runs - s0.effect_runs,
                    s1.propagations - s0.propagations,
                    s1.flushes - s0.flushes,
                );
            }
        }
        if i == a.warm {
            before = Some(app.engine().graph().stats());
        }
        clock.advance(period);
        let mut events = Vec::new();
        if let Some(p) = press.take() {
            events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        if let Some(p) = click_at(i) {
            events.push(egui::Event::PointerMoved(p));
            events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            press = Some(p);
        }
        let input = egui::RawInput {
            screen_rect: Some(screen(i)),
            focused: true,
            time: Some(f64::from(i) * period.as_secs_f64()),
            events,
            ..Default::default()
        };
        let t0 = Instant::now();
        let mut out = ctx.run_ui(input, |ui| {
            app.begin_pass();
            app.show(ui);
        });
        // There is no painter to take the textures; a debug build of
        // epaint asserts that a delta is not dropped unapplied.
        out.textures_delta.clear();
        let t1 = Instant::now();
        let soon = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay <= period);
        let p = ctx.tessellate(out.shapes, out.pixels_per_point);
        let t2 = Instant::now();
        if i >= a.warm {
            pass += t1 - t0;
            tess += t2 - t1;
            if a.slow.is_some_and(|s| t2 - t0 > s) {
                println!("frame {}: {:.2?}", i - a.warm, t2 - t0);
            }
            if t2 - t0 > worst {
                (worst, worst_at) = (t2 - t0, i - a.warm);
            }
            prims += p.len();
            eager += u32::from(soon);
        }
    }
    let ms = |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(a.frames);
    let stats = app.view().stats();
    println!(
        "{} frames: pass {:.3} ms, tessellate {:.3} ms, total {:.3} ms/frame (worst {:.2} ms, frame {}); {} scene frames, {} changed, {} asked for the next; {:.0} primitives/frame",
        a.frames,
        ms(pass),
        ms(tess),
        ms(pass + tess),
        worst.as_secs_f64() * 1000.0,
        worst_at,
        stats.frames,
        stats.changed,
        eager,
        prims as f64 / f64::from(a.frames.max(1)),
    );
    if let Some(b) = before {
        let s = app.engine().graph().stats();
        let per = |x: u64, y: u64| (x - y) as f64 / f64::from(a.frames);
        println!(
            "per frame: {:.1} evaluations, {:.1} effect runs, {:.1} propagations, {:.2} flushes",
            per(s.evaluations, b.evaluations),
            per(s.effect_runs, b.effect_runs),
            per(s.propagations, b.propagations),
            per(s.flushes, b.flushes),
        );
    }
    for w in app.engine().take_warnings().iter().take(10) {
        eprintln!("warning: {w}");
    }
    if let Some(root) = root.get() {
        for e in &a.eval {
            let said = app.engine().eval_js_in(
                Some(root),
                e,
                Duration::from_secs(1),
                [4, 32, 200, 1000, 4000],
            );
            match said {
                Ok(v) => println!("{e}: {v}"),
                Err(err) => println!("{e}: {err}"),
            }
        }
    }
    Ok(())
}
