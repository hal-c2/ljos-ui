//! What a hot reload costs, without a window: a watched QML project is
//! shown in a `ShellApp` on an `egui::Context`, an edit is saved again
//! and again, and each reload is told with its build, its apply, the time
//! from the save to the first pass that shows it, and the process's
//! resident memory after it. The edit swaps `OLD` for `NEW` in `FILE` and
//! back, so every save changes the file.
//!
//! `cargo run --release -p ljos_shell --features dev --example reload_profile --
//! DIR ENTRY FILE OLD NEW [--times N] [--size WxH] [--eval EXPR]…`
//!
//! `--eval` runs `EXPR` in the root's context once the project has
//! settled, before the first save: what puts the application in a state
//! worth keeping (a game started, a page pushed). Run it on a copy of the
//! project: the file is left as it was at the start.
//!
//! `LJOS_LOG=ljos_engine::incubator::reload=debug` also tells how long
//! each step of every reload took.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use ljos_runtime::SystemClock;
use ljos_shell::{Report, ShellApp, ShellConfig};

struct Args {
    dir: PathBuf,
    entry: String,
    file: String,
    old: String,
    new: String,
    times: u32,
    size: (f32, f32),
    eval: Vec<String>,
}

fn args() -> anyhow::Result<Args> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        dir: PathBuf::from(it.next().context("DIR")?),
        entry: it.next().context("ENTRY")?,
        file: it.next().context("FILE")?,
        old: it.next().context("OLD")?,
        new: it.next().context("NEW")?,
        times: 10,
        size: (800.0, 600.0),
        eval: Vec::new(),
    };
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--times" => a.times = it.next().context("N")?.parse()?,
            "--eval" => a.eval.push(it.next().context("EXPR")?),
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

fn rss_kb() -> u64 {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: u64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    pages * 4
}

fn main() -> anyhow::Result<()> {
    let a = args()?;
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::WARN.into())
                .with_env_var("LJOS_LOG")
                .from_env_lossy(),
        )
        .init();
    let dir = a.dir.canonicalize()?;
    let main = dir.join(&a.entry);
    let path = dir.join(&a.file);
    let original = std::fs::read_to_string(&path)?;
    if !original.contains(&a.old) {
        bail!("{} has no {:?}", a.file, a.old);
    }
    let ctx = egui::Context::default();
    let config = ShellConfig::new("reload profile").with_size(a.size.0, a.size.1);
    let reports: Rc<RefCell<Vec<Report>>> = Rc::default();
    let heard = reports.clone();
    let mut app = ShellApp::<()>::new(&ctx, &config, Arc::new(SystemClock::new()), |setup| {
        ljos_localstorage::register(setup.engine());
        ljos_localstorage::set_storage(setup.engine(), ljos_localstorage::Storage::Memory);
        let root = setup.watch(&main)?;
        setup.on_reload(move |r| heard.borrow_mut().push(r.clone()));
        Ok(root)
    })?;
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(a.size.0, a.size.1));
    let start = Instant::now();
    let mut pass = |app: &mut ShellApp<()>| {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            focused: true,
            time: Some(start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| {
            app.begin_pass();
            app.show(ui);
        });
        let _ = ctx.tessellate(out.shapes, out.pixels_per_point);
    };
    let settle = |app: &mut ShellApp<()>, pass: &mut dyn FnMut(&mut ShellApp<()>), d: Duration| {
        let until = Instant::now() + d;
        while Instant::now() < until {
            pass(app);
            std::thread::sleep(Duration::from_millis(4));
        }
    };
    settle(&mut app, &mut pass, Duration::from_millis(1500));
    if let Some(root) = app.view().scene().root() {
        for e in &a.eval {
            let said = app.engine().eval_js_in(
                Some(root),
                e,
                Duration::from_secs(5),
                [4, 32, 200, 1000, 4000],
            );
            println!(
                "{e}: {}",
                said.map_or_else(|e| e.to_string(), |v| v.to_string())
            );
            settle(&mut app, &mut pass, Duration::from_millis(500));
        }
    }
    settle(&mut app, &mut pass, Duration::from_millis(1500));
    let _ = app.engine().take_warnings();
    println!("rss before {} kB", rss_kb());
    let mut shown_all = Vec::new();
    for i in 0..a.times {
        let text = if i % 2 == 0 {
            original.replacen(&a.old, &a.new, 1)
        } else {
            original.clone()
        };
        std::fs::write(&path, text)?;
        let until = Instant::now() + Duration::from_secs(10);
        let stats = loop {
            pass(&mut app);
            let got = reports.borrow_mut().drain(..).collect::<Vec<_>>();
            let mut done = None;
            for r in got {
                match r {
                    Report::Reloaded(s) => done = Some(s),
                    other => println!("  {other:?}"),
                }
            }
            if let Some(s) = done {
                break Some(s);
            }
            if Instant::now() > until {
                break None;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let Some(s) = stats else {
            println!("reload {i}: nothing came in 10 s");
            continue;
        };
        settle(&mut app, &mut pass, Duration::from_millis(300));
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let shown = s.shown.map_or(f64::NAN, ms);
        shown_all.push(shown);
        println!(
            "reload {i}: build {:.1} ms, apply {:.1} ms, shown {:.1} ms; {} kept, {} new, {} made again; rss {} kB",
            ms(s.build),
            ms(s.apply),
            shown,
            s.kept,
            s.fresh,
            s.remounted,
            rss_kb(),
        );
        for w in app.engine().take_warnings().iter().take(5) {
            println!("  warning: {w}");
        }
    }
    std::fs::write(&path, &original)?;
    shown_all.sort_by(f64::total_cmp);
    if !shown_all.is_empty() {
        println!(
            "shown: median {:.1} ms, min {:.1} ms, max {:.1} ms",
            shown_all[shown_all.len() / 2],
            shown_all[0],
            shown_all[shown_all.len() - 1]
        );
    }
    Ok(())
}
