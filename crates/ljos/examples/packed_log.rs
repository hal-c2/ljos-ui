//! An application whose QML comes compiled in a pack, run headlessly,
//! and where what it logs goes: the four choices an application has.
//! A shipped application embeds its pack (`ljos::include_qml!`); this one
//! reads the pack from a file, so that one program runs any QML. The
//! scenarios of `features/tooling/packed-log.feature` run it.
//!
//! ```text
//! cargo run -p ljos --example packed_log -- app.pack Main [stderr|own|setup|kept]
//! ```
//!
//! - `stderr`, the default: the log goes to stderr, a line each after its
//!   level (`debug: hello`), as `ljos run` writes it.
//! - `own`: a handler of the application's own ([`ljos::App::on_log`])
//!   writes each entry to stdout (`heard debug: hello`) instead.
//! - `setup`: the same handler, set on the Engine in the setup, wins over
//!   the builder's choice of stderr.
//! - `kept`: nothing is written while it runs ([`ljos::App::log_to_stderr`]
//!   with `false`); the Engine keeps the log, and at the end the program
//!   says on stdout how many entries it kept.

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use ljos::prelude::*;

/// The application's own handler: its log on stdout.
fn heard(entry: &LogEntry) {
    println!("heard {}", entry.line());
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [pack, root, rest @ ..] = args.as_slice() else {
        eprintln!("usage: packed_log PACK ROOT [stderr|own|setup|kept]");
        return ExitCode::from(2);
    };
    let source = match std::fs::read(pack)
        .map_err(|e| e.to_string())
        .and_then(|bytes| ljos::MapSource::from_pack(&bytes).map_err(|e| e.to_string()))
    {
        Ok(source) => source,
        Err(e) => {
            eprintln!("packed_log: {pack}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let app = ljos::App::new("Packed").source(source);
    let root = root.clone();
    let kept: Rc<RefCell<Option<Rc<Engine>>>> = Rc::default();
    let ran = match rest.first().map(String::as_str).unwrap_or("stderr") {
        "stderr" => app.root(root).run_headless(),
        "own" => app.root(root).on_log(heard).run_headless(),
        "setup" => app.log_to_stderr(true).run_headless_with(move |setup| {
            setup.engine().set_log_handler(heard);
            make(setup, &root)
        }),
        "kept" => {
            let keep = Rc::clone(&kept);
            app.log_to_stderr(false).run_headless_with(move |setup| {
                *keep.borrow_mut() = Some(Rc::clone(setup.engine()));
                make(setup, &root)
            })
        }
        other => {
            eprintln!("packed_log: no log choice {other:?}: stderr, own, setup or kept");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = ran {
        eprintln!("packed_log: {e}");
        return ExitCode::FAILURE;
    }
    if let Some(engine) = kept.borrow_mut().take() {
        println!("kept {} entries", engine.take_log().len());
    }
    ExitCode::SUCCESS
}

fn make(setup: &mut ljos::Setup<'_, ()>, root: &str) -> Result<ObjectId, ljos::SetupError> {
    setup
        .engine()
        .create_from_url(root, None, Vec::new())
        .map_err(|e| -> ljos::SetupError { e.to_string().into() })
}
