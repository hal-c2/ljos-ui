//! Whether a hot reload makes what a fresh load makes: a watched QML
//! project is shown in a `ShellApp` without a window, a series of random
//! small edits is saved to its files (an object added, removed or moved,
//! a binding, a handler or a function changed, a property renamed), and
//! after each one the running application is compared with the same files
//! loaded afresh in another `ShellApp`: the tree of objects, every
//! property's value and whether it is bound.
//!
//! `cargo run --release -p ljos_shell --features dev --example edit_fuzz --
//! DIR ENTRY [--edits N] [--seed S] [--size WxH]`
//!
//! Nothing is clicked or typed and the clocks run: what the reload keeps
//! is what the application made of itself since it was loaded, which a
//! fresh load makes again. A property that changes by itself (an
//! animation, a timer, the time of day) is left out of the comparison:
//! one that two looks at the same application 1.1 s apart disagree on,
//! or two fresh loads do (`Math.random()`).
//!
//! Run it on a copy of the project: the files are edited, and put back as
//! they were at the end.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use ljos_engine::{Engine, ObjectId, Provenance, Value};
use ljos_runtime::SystemClock;
use ljos_shell::{Report, ShellApp, ShellConfig};

struct Args {
    dir: PathBuf,
    entry: String,
    edits: u32,
    seed: u64,
    size: (f32, f32),
    /// Only load, and print what the comparison would look at.
    dump: bool,
    /// Stop after this edit, leave the files as it left them and print
    /// every difference.
    stop_at: Option<u32>,
    /// Save each file's text over the project's file of that path, one
    /// after the other, and print every difference after the last: edits
    /// made by hand.
    save: Vec<(String, PathBuf)>,
    /// Keep each edit's text here, to be saved again with `--save`.
    trace: Option<PathBuf>,
}

fn args() -> anyhow::Result<Args> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        dir: PathBuf::from(it.next().context("DIR")?),
        entry: it.next().context("ENTRY")?,
        edits: 40,
        seed: 1,
        size: (800.0, 600.0),
        dump: false,
        stop_at: None,
        save: Vec::new(),
        trace: None,
    };
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--edits" => a.edits = it.next().context("N")?.parse()?,
            "--seed" => a.seed = it.next().context("S")?.parse()?,
            "--dump" => a.dump = true,
            "--save" => {
                let rel = it.next().context("REL")?;
                a.save
                    .push((rel, PathBuf::from(it.next().context("TEXT")?)));
            }
            "--trace" => a.trace = Some(PathBuf::from(it.next().context("DIR")?)),
            "--stop-at" => a.stop_at = Some(it.next().context("N")?.parse()?),
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

/// The engine's debug lines, for why a reload did not go in place.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Log {
    fn take(&self) -> String {
        let bytes = std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

// ------------------------------------------------------------------ apps

struct App {
    ctx: egui::Context,
    app: ShellApp<()>,
    reports: Rc<RefCell<Vec<Report>>>,
    start: Instant,
    size: (f32, f32),
}

impl App {
    fn open(main: &Path, size: (f32, f32)) -> anyhow::Result<Self> {
        let ctx = egui::Context::default();
        let config = ShellConfig::new("edit fuzz").with_size(size.0, size.1);
        let reports: Rc<RefCell<Vec<Report>>> = Rc::default();
        let heard = reports.clone();
        let app = ShellApp::<()>::new(&ctx, &config, Arc::new(SystemClock::new()), |setup| {
            ljos_localstorage::register(setup.engine());
            ljos_localstorage::set_storage(setup.engine(), ljos_localstorage::Storage::Memory);
            ljos_xml::register(setup.engine());
            ljos_folderlistmodel::register(setup.engine());
            let root = setup.watch(main)?;
            setup.on_reload(move |r| heard.borrow_mut().push(r.clone()));
            Ok(root)
        })?;
        Ok(Self {
            ctx,
            app,
            reports,
            start: Instant::now(),
            size,
        })
    }

    fn pass(&mut self) {
        let screen =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(self.size.0, self.size.1));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            focused: true,
            time: Some(self.start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| {
            app.begin_pass();
            app.show(ui);
        });
        out.textures_delta.clear();
        let _ = self.ctx.tessellate(out.shapes, out.pixels_per_point);
    }

    fn settle(&mut self, d: Duration) {
        let until = Instant::now() + d;
        while Instant::now() < until {
            self.pass();
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    fn engine(&self) -> &Engine {
        self.app.engine()
    }

    fn root(&self) -> Option<ObjectId> {
        self.app.view().scene().root()
    }

    /// Passes until the watcher brings what was saved; None when nothing
    /// came (the text compiles to what runs).
    fn await_report(&mut self, within: Duration) -> Option<Report> {
        let until = Instant::now() + within;
        loop {
            self.pass();
            let got: Vec<Report> = self.reports.borrow_mut().drain(..).collect();
            if let Some(r) = got
                .into_iter()
                .find(|r| matches!(r, Report::Reloaded(_) | Report::Failed { .. }))
            {
                return Some(r);
            }
            if Instant::now() > until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

// ------------------------------------------------------------------ dump

/// What one object is, by its path from the root: its type, its id and
/// each property's value, `=` when it holds a value, `:` when bound and
/// `~` when an animation or a Behavior wrote it last.
type Dump = BTreeMap<String, String>;

/// As the inspector walks the tree: the children an object owns that are
/// not shown under another item, then the items shown in it.
fn below(engine: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = engine
        .children(obj)
        .into_iter()
        .filter(|c| engine.visual_parent(*c).is_none_or(|p| !engine.is_alive(p)))
        .collect();
    out.extend(engine.visual_children(obj));
    out
}

fn type_of(engine: &Engine, obj: ObjectId) -> String {
    engine
        .meta_of(obj)
        .map_or_else(|| "?".to_owned(), |m| m.name.to_string())
}

fn dump(engine: &Engine, root: ObjectId) -> Dump {
    let mut paths: HashMap<ObjectId, String> = HashMap::new();
    let mut order = Vec::new();
    let mut stack = vec![(root, type_of(engine, root))];
    while let Some((obj, path)) = stack.pop() {
        if paths.contains_key(&obj) {
            continue;
        }
        paths.insert(obj, path.clone());
        order.push(obj);
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut kids = Vec::new();
        for c in below(engine, obj) {
            let ty = type_of(engine, c);
            let n = seen.entry(ty.clone()).or_default();
            kids.push((c, format!("{path}/{ty}[{n}]")));
            *n += 1;
        }
        stack.extend(kids.into_iter().rev());
    }
    let ids = engine.inspect_ids(&order);
    let mut out = Dump::new();
    for (obj, id) in order.iter().zip(ids) {
        let path = &paths[obj];
        if let Some(id) = id {
            out.insert(format!("{path} id"), id);
        }
        let Some(meta) = engine.meta_of(*obj) else {
            continue;
        };
        for (i, p) in meta.properties.iter().enumerate() {
            let prop = ljos_engine::PropIndex(i as u32);
            let v = show(&engine.read(*obj, prop), &paths, engine);
            let how = match engine.provenance(*obj, prop) {
                Some(Provenance::Animated) => '~',
                _ if engine.is_bound(*obj, prop) => ':',
                _ => '=',
            };
            out.insert(format!("{path} .{}", p.name), format!("{how} {v}"));
        }
    }
    out
}

fn show(v: &Value, paths: &HashMap<ObjectId, String>, engine: &Engine) -> String {
    match v {
        Value::Real(r) => format!("{r:.3}"),
        Value::Object(o) => match paths.get(o) {
            Some(p) => format!("@{p}"),
            None if engine.is_alive(*o) => format!("@<{}>", type_of(engine, *o)),
            None => "@dead".to_owned(),
        },
        Value::List(items) => {
            let shown: Vec<String> = items.iter().map(|v| show(v, paths, engine)).collect();
            format!("[{}]", shown.join(", "))
        }
        Value::Js(_) => "js".to_owned(),
        other => format!("{other:?}"),
    }
}

/// The keys two dumps of one application disagree on.
fn moving(a: &Dump, b: &Dump) -> BTreeSet<String> {
    a.iter()
        .filter(|(k, v)| b.get(*k).is_some_and(|w| w != *v))
        .map(|(k, _)| k.clone())
        .collect()
}

fn differences(live: &Dump, fresh: &Dump, skip: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in fresh {
        if skip.contains(k) {
            continue;
        }
        match live.get(k) {
            None => out.push(format!("only fresh: {k} {v}")),
            Some(w) if w != v => out.push(format!("{k}: live {w}, fresh {v}")),
            Some(_) => {}
        }
    }
    for (k, v) in live {
        if !fresh.contains_key(k) && !skip.contains(k) {
            out.push(format!("only live: {k} {v}"));
        }
    }
    out
}

// ----------------------------------------------------------------- edits

/// An object written in a file: the lines it spans, its type and whether
/// it is given to a member (`delegate: Item {`).
#[derive(Debug, Clone)]
struct Block {
    start: usize,
    end: usize,
    ty: String,
    member: bool,
    depth: usize,
}

/// The column after the `}` that closes the `{` at `(line, col)`,
/// skipping strings and comments; None when it is not closed.
fn close_of(lines: &[&str], line: usize, col: usize) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    let mut in_block_comment = false;
    for (l, text) in lines.iter().enumerate().skip(line) {
        let b = text.as_bytes();
        let mut i = if l == line { col } else { 0 };
        let mut quote: Option<u8> = None;
        while i < b.len() {
            let c = b[i];
            if in_block_comment {
                if c == b'*' && b.get(i + 1) == Some(&b'/') {
                    in_block_comment = false;
                    i += 1;
                }
            } else if let Some(q) = quote {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
                break;
            } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
                in_block_comment = true;
                i += 1;
            } else if c == b'"' || c == b'\'' || c == b'`' {
                quote = Some(c);
            } else if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Some((l, i + 1));
                }
            }
            i += 1;
        }
    }
    None
}

fn header(text: &str) -> Option<(String, bool)> {
    let t = text.trim_start();
    let (member, rest) = match t.split_once(':') {
        Some((name, rest))
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') =>
        {
            (true, rest.trim_start())
        }
        _ => (false, t),
    };
    let ty: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let after = rest[ty.len()..].trim_start();
    let first = ty.rsplit('.').next()?.chars().next()?;
    (first.is_ascii_uppercase() && after.starts_with('{') && !ty.starts_with("on"))
        .then_some((ty, member))
}

fn blocks(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Block> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let Some((ty, member)) = header(l) else {
            continue;
        };
        let Some(col) = l.find('{') else { continue };
        let Some((end, after)) = close_of(&lines, i, col) else {
            continue;
        };
        // Whole lines only: what follows the `}` is not cut with it.
        if !lines[end][after..].trim().is_empty() {
            continue;
        }
        let depth = out.iter().filter(|b| b.start < i && b.end >= end).count();
        out.push(Block {
            start: i,
            end,
            ty,
            member,
            depth,
        });
    }
    out
}

/// Types that hold no items: nothing is added to them.
const NOT_ITEMS: &[&str] = &[
    "ListModel",
    "ListElement",
    "State",
    "PropertyChanges",
    "Transition",
    "Behavior",
    "NumberAnimation",
    "PropertyAnimation",
    "ColorAnimation",
    "SequentialAnimation",
    "ParallelAnimation",
    "Timer",
    "Connections",
    "Binding",
    "Component",
    "Gradient",
    "GradientStop",
    "FontLoader",
    "Shortcut",
    "XmlListModel",
    "XmlListModelRole",
    "Repeater",
    "ListView",
    "GridView",
    "PathView",
    "Loader",
    "Scale",
    "Rotation",
    "Translate",
    "SpringAnimation",
    "SmoothedAnimation",
    "ScriptAction",
    "PauseAnimation",
    "AnchorChanges",
    "StateChangeScript",
    "Font",
];

fn indent_of(s: &str) -> String {
    s.chars().take_while(|c| c.is_whitespace()).collect()
}

fn pick<'a, T>(rng: &mut Rng, v: &'a [T]) -> Option<&'a T> {
    (!v.is_empty()).then(|| &v[rng.below(v.len())])
}

/// One random edit of `kind` to `text`, and what it did; None when the
/// file has nothing to do it to.
fn edit(kind: usize, text: &str, rng: &mut Rng, n: u32) -> Option<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_owned()).collect();
    let bs = blocks(text);
    let join = |out: Vec<String>| out.join("\n") + "\n";
    match kind {
        // An object added to one that holds items.
        0 => {
            let into: Vec<&Block> = bs
                .iter()
                .filter(|b| !NOT_ITEMS.contains(&b.ty.as_str()) && b.end > b.start)
                .collect();
            let b = *pick(rng, &into)?;
            let ind = indent_of(lines[b.start]) + "    ";
            out.insert(
                b.end,
                format!("{ind}Item {{ objectName: \"fuzz{n}\"; width: {n}; height: 2 }}"),
            );
            Some((
                join(out),
                format!("add an Item to the {} at line {}", b.ty, b.start + 1),
            ))
        }
        // An object removed.
        1 => {
            let some: Vec<&Block> = bs.iter().filter(|b| b.depth > 0).collect();
            let b = *pick(rng, &some)?;
            out.drain(b.start..=b.end);
            Some((
                join(out),
                format!("remove the {} at line {}", b.ty, b.start + 1),
            ))
        }
        // An object moved: before a sibling, or into another object.
        2 => {
            let some: Vec<&Block> = bs.iter().filter(|b| b.depth > 0 && !b.member).collect();
            let b = (*pick(rng, &some)?).clone();
            let cut: Vec<String> = out.drain(b.start..=b.end).collect();
            let len = cut.len();
            let shift = |at: usize| if at > b.end { at - len } else { at };
            let siblings: Vec<&Block> = bs
                .iter()
                .filter(|s| {
                    s.depth == b.depth
                        && !s.member
                        && s.start < b.start
                        && !bs.iter().any(|p| {
                            p.depth + 1 == b.depth
                                && p.start < b.start
                                && p.end > b.end
                                && !(p.start < s.start && p.end > s.end)
                        })
                })
                .collect();
            let into: Vec<&Block> = bs
                .iter()
                .filter(|p| {
                    !NOT_ITEMS.contains(&p.ty.as_str())
                        && p.end > p.start
                        && !(p.start >= b.start && p.end <= b.end)
                })
                .collect();
            if rng.below(2) == 0
                && let Some(s) = pick(rng, &siblings)
            {
                let at = shift(s.start);
                for (k, l) in cut.into_iter().enumerate() {
                    out.insert(at + k, l);
                }
                Some((
                    join(out),
                    format!(
                        "move the {} at line {} before the {} at line {}",
                        b.ty,
                        b.start + 1,
                        s.ty,
                        s.start + 1
                    ),
                ))
            } else {
                let p = *pick(rng, &into)?;
                let at = shift(p.end);
                for (k, l) in cut.into_iter().enumerate() {
                    out.insert(at + k, l);
                }
                Some((
                    join(out),
                    format!(
                        "move the {} at line {} into the {} at line {}",
                        b.ty,
                        b.start + 1,
                        p.ty,
                        p.start + 1
                    ),
                ))
            }
        }
        // A binding's value changed: a number, a string or a truth.
        3 => {
            let some: Vec<usize> = (0..lines.len())
                .filter(|&i| {
                    let t = lines[i].trim_start();
                    let Some((name, rest)) = t.split_once(':') else {
                        return false;
                    };
                    let name_ok = !name.is_empty()
                        && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
                        && name != "id"
                        && !(name.starts_with("on")
                            && name[2..]
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_uppercase()));
                    name_ok && header(lines[i]).is_none() && literal(rest).is_some()
                })
                .collect();
            let &i = pick(rng, &some)?;
            let (name, rest) = lines[i].split_once(':')?;
            let changed = literal(rest)?;
            out[i] = format!("{name}:{changed}");
            Some((
                join(out),
                format!("change {} at line {}", name.trim(), i + 1),
            ))
        }
        // A handler's body changed.
        4 => {
            let some: Vec<usize> = (0..lines.len())
                .filter(|&i| {
                    let t = lines[i].trim_start();
                    t.starts_with("on")
                        && t[2..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                        && t.split_once(':').is_some_and(|(n, r)| {
                            !n.contains(' ') && !r.trim().is_empty() && header(lines[i]).is_none()
                        })
                })
                .collect();
            let &i = pick(rng, &some)?;
            let (name, rest) = lines[i].split_once(':')?;
            let rest = rest.trim();
            if let Some(body) = rest.strip_prefix('{') {
                out[i] = format!("{name}: {{ var fuzz{n} = {n};{body}");
            } else if rest.starts_with("function")
                || rest.starts_with('(')
                || rest.ends_with('{')
                || rest.ends_with(',')
            {
                // Its body goes on below: not one line to wrap.
                return None;
            } else {
                out[i] = format!("{name}: {{ var fuzz{n} = {n}; {rest} }}");
            }
            Some((
                join(out),
                format!("change the handler {} at line {}", name.trim(), i + 1),
            ))
        }
        // A function's body changed: a number in it, or a statement more.
        5 => {
            let some: Vec<usize> = (0..lines.len())
                .filter(|&i| {
                    let t = lines[i].trim_start();
                    t.starts_with("function ") && t.trim_end().ends_with('{')
                })
                .collect();
            let &i = pick(rng, &some)?;
            let col = lines[i].rfind('{')?;
            let (end, _) = close_of(&lines, i, col)?;
            let numbered: Vec<usize> = (i + 1..end)
                .filter(|&k| number(lines[k]).is_some())
                .collect();
            let what = if rng.below(2) == 0
                && let Some(&k) = pick(rng, &numbered)
            {
                out[k] = number(lines[k])?;
                format!("a number at line {}", k + 1)
            } else {
                let ind = indent_of(lines[i]) + "    ";
                out.insert(i + 1, format!("{ind}var fuzz{n} = {n};"));
                "a statement".to_owned()
            };
            Some((
                join(out),
                format!("change {what} in the function at line {}", i + 1),
            ))
        }
        // A property renamed, and every use of it in the file.
        _ => {
            let some: Vec<(usize, String)> = (0..lines.len())
                .filter_map(|i| {
                    let mut words = lines[i].split_whitespace().peekable();
                    while words
                        .peek()
                        .is_some_and(|w| matches!(*w, "readonly" | "required" | "default"))
                    {
                        words.next();
                    }
                    if words.next()? != "property" {
                        return None;
                    }
                    let _ty = words.next()?;
                    let name: String = words
                        .next()?
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    (!name.is_empty() && lines[i].contains("property")).then_some((i, name))
                })
                .collect();
            let (i, name) = pick(rng, &some)?.clone();
            let new = format!("{name}Fz{n}");
            let cap = |s: &str| {
                let mut c = s.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            };
            let text = rename(text, &name, &new);
            let text = rename(
                &text,
                &format!("on{}Changed", cap(&name)),
                &format!("on{}Changed", cap(&new)),
            );
            let text = rename(&text, &format!("{name}Changed"), &format!("{new}Changed"));
            Some((
                text,
                format!("rename the property {name} at line {} to {new}", i + 1),
            ))
        }
    }
}

/// `rest` with its first literal changed; None when it has none.
fn literal(rest: &str) -> Option<String> {
    let t = rest.trim();
    if t == "true" {
        return Some(rest.replacen("true", "false", 1));
    }
    if t == "false" {
        return Some(rest.replacen("false", "true", 1));
    }
    if let Some(s) = number(rest) {
        return Some(s);
    }
    let q = rest.find('"')?;
    let r = rest[q + 1..].find('"')?;
    Some(format!(
        "{}\"{}z\"{}",
        &rest[..q],
        &rest[q + 1..q + 1 + r],
        &rest[q + 2 + r..]
    ))
}

/// `s` with its first number that stands alone one more.
fn number(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    // Not in a comment, a string or a name.
    let mut quote = None;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == b'"' || c == b'\'' {
            quote = Some(c);
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            return None;
        } else if c.is_ascii_digit()
            && (i == 0
                || !(b[i - 1].is_ascii_alphanumeric()
                    || b[i - 1] == b'_'
                    || b[i - 1] == b'.'
                    || b[i - 1] == b'#'))
        {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if b.get(j)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            {
                i = j;
                continue;
            }
            let v: u64 = s[i..j].parse().ok()?;
            return Some(format!("{}{}{}", &s[..i], v + 1, &s[j..]));
        }
        i += 1;
    }
    None
}

/// `text` with every `old` that stands as a word of its own made `new`.
fn rename(text: &str, old: &str, new: &str) -> String {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(old) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + old.len()..].chars().next();
        out.push_str(&rest[..at]);
        if before.is_some_and(word) || after.is_some_and(word) {
            out.push_str(old);
        } else {
            out.push_str(new);
        }
        rest = &rest[at + old.len()..];
    }
    out.push_str(rest);
    out
}

/// From a load or a reload to the first look: what starts moving at the
/// start (a needle swinging to the time) has come to rest.
const SETTLE: Duration = Duration::from_millis(2000);
/// Between two looks: longer than a second, so a clock's tick shows as
/// moving.
const APART: Duration = Duration::from_millis(1100);

const KINDS: [&str; 7] = [
    "add", "remove", "move", "binding", "handler", "function", "rename",
];

fn qml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "qml") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn write_out(path: &Path, text: &str) -> std::io::Result<()> {
    // Renamed into place: the watcher sees one whole file.
    let tmp = path.with_extension("qml.fuzz~");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

/// A fresh load's two looks, `APART` from each other, and whether its
/// scripts threw.
fn look(main: &Path, size: (f32, f32)) -> anyhow::Result<(Dump, Dump, bool)> {
    let mut fresh = App::open(main, size)?;
    fresh.settle(SETTLE);
    let f1 = fresh
        .root()
        .map(|r| dump(fresh.engine(), r))
        .unwrap_or_default();
    fresh.settle(APART);
    let f2 = fresh
        .root()
        .map(|r| dump(fresh.engine(), r))
        .unwrap_or_default();
    let threw = fresh
        .engine()
        .take_warnings()
        .iter()
        .any(|w| w.to_string().contains("Error"));
    Ok((f1, f2, threw))
}

fn main() -> anyhow::Result<()> {
    let a = args()?;
    let log = Log::default();
    let sink = log.clone();
    tracing_subscriber::fmt()
        .with_writer(move || sink.clone())
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "error,ljos_engine::incubator::reload=debug",
        ))
        .init();
    let dir = a.dir.canonicalize()?;
    let main = dir.join(&a.entry);
    if a.dump {
        let mut app = App::open(&main, a.size)?;
        app.settle(SETTLE);
        for w in app.engine().take_warnings() {
            println!("warning: {w}");
        }
        for (k, v) in app
            .root()
            .map(|r| dump(app.engine(), r))
            .unwrap_or_default()
        {
            println!("{k} {v}");
        }
        return Ok(());
    }
    if !a.save.is_empty() {
        let files = qml_files(&dir);
        let originals: Vec<(PathBuf, String)> = files
            .iter()
            .map(|p| Ok((p.clone(), std::fs::read_to_string(p)?)))
            .collect::<anyhow::Result<_>>()?;
        let mut live = App::open(&main, a.size)?;
        live.settle(SETTLE);
        for (rel, text) in &a.save {
            write_out(&dir.join(rel), &std::fs::read_to_string(text)?)?;
            let got = live.await_report(Duration::from_secs(4));
            println!("{rel}: {got:?}");
            for l in log.take().lines().filter(|l| l.contains("reload")) {
                println!("log: {l}");
            }
            live.settle(SETTLE);
        }
        let l2 = live
            .root()
            .map(|r| dump(live.engine(), r))
            .unwrap_or_default();
        let (f1, f2, _) = look(&main, a.size)?;
        for (p, text) in &originals {
            std::fs::write(p, text)?;
        }
        for d in differences(&l2, &f2, &moving(&f1, &f2)) {
            println!("    {d}");
        }
        return Ok(());
    }
    let files = qml_files(&dir);
    let originals: Vec<(PathBuf, String)> = files
        .iter()
        .map(|p| Ok((p.clone(), std::fs::read_to_string(p)?)))
        .collect::<anyhow::Result<_>>()?;
    let mut rng = Rng(a.seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut live = App::open(&main, a.size)?;
    live.settle(Duration::from_millis(1500));
    let _ = log.take();
    // An application that does not load as it is (a module or a type made
    // in C++ that is not here) says nothing about reloading.
    let failed = live.reports.borrow_mut().drain(..).find_map(|r| match r {
        Report::Failed { fault, .. } => Some(fault.message.clone()),
        _ => None,
    });
    // A first load that fails is shown as an empty item and told to no
    // one, so the files are saved once as they are: the build of that save
    // fails as the load did.
    let failed = match failed {
        Some(why) => Some(why),
        None => {
            let text = std::fs::read_to_string(&main)?;
            write_out(&main, &format!("{text}\n"))?;
            let got = live.await_report(Duration::from_secs(4));
            write_out(&main, &text)?;
            let _ = live.await_report(Duration::from_secs(4));
            match got {
                Some(Report::Failed { fault, .. }) => Some(fault.message.clone()),
                _ => None,
            }
        }
    };
    if let Some(why) = failed {
        writeln!(std::io::stdout(), "== does not load: {why}")?;
        return Ok(());
    }
    // What the running application holds before each edit, and what a
    // fresh load of the same files holds: a value the edit did not touch
    // that the first has and the second has not is state the application
    // built up while it ran, which a reload keeps.
    let mut was_live = live
        .root()
        .map(|r| dump(live.engine(), r))
        .unwrap_or_default();
    let (_, mut was_fresh, mut was_threw) = look(&main, a.size)?;
    let _ = log.take();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut kinds_full: BTreeMap<&str, usize> = BTreeMap::new();
    let mut stdout = std::io::stdout().lock();
    let mut tried = 0;
    let mut n = 0u32;
    while n < a.edits && tried < a.edits * 20 {
        tried += 1;
        let kind = rng.below(KINDS.len());
        let path = &files[rng.below(files.len())];
        let before = std::fs::read_to_string(path)?;
        let Some((after, what)) = edit(kind, &before, &mut rng, n + 1) else {
            continue;
        };
        if after == before {
            continue;
        }
        n += 1;
        let file = path
            .strip_prefix(&dir)
            .unwrap_or(path)
            .display()
            .to_string();
        let root_before = live.root();
        if let Some(t) = &a.trace {
            std::fs::create_dir_all(t)?;
            std::fs::write(t.join(format!("{n:02}-{}", file.replace('/', "%"))), &after)?;
        }
        write_out(path, &after)?;
        let got = live.await_report(Duration::from_secs(4));
        let said = log.take();
        let outcome = match got {
            None => {
                *counts.entry("no reload").or_default() += 1;
                writeln!(stdout, "#{n} {file}: {what}: nothing to reload")?;
                continue;
            }
            Some(Report::Failed { fault, .. }) => {
                *counts.entry("did not compile").or_default() += 1;
                writeln!(
                    stdout,
                    "#{n} {file}: {what}: does not compile: {}",
                    fault.message
                )?;
                write_out(path, &before)?;
                let _ = live.await_report(Duration::from_secs(4));
                let _ = log.take();
                continue;
            }
            Some(Report::Reloaded(s)) => s,
            Some(_) => unreachable!(),
        };
        let full = said.contains("reload steps: snapshot") || live.root() != root_before;
        let why: Vec<String> = said
            .lines()
            .filter_map(|l| {
                l.split_once("reload not in place: ")
                    .map(|(_, w)| w.to_owned())
            })
            .collect();
        if full {
            *counts.entry("full path").or_default() += 1;
            *kinds_full.entry(KINDS[kind]).or_default() += 1;
            let why = why
                .first()
                .cloned()
                .unwrap_or_else(|| "(no reason told)".to_owned());
            // The op's fields beyond its kind name places, not reasons.
            let short: String = why.chars().take_while(|c| *c != '{').collect();
            *reasons.entry(short.trim().to_owned()).or_default() += 1;
        } else {
            *counts.entry("in place").or_default() += 1;
        }
        live.settle(SETTLE);
        let Some(root) = live.root() else {
            writeln!(stdout, "#{n} {file}: {what}: no root after the reload")?;
            continue;
        };
        let l1 = dump(live.engine(), root);
        live.settle(APART);
        let root = live.root().context("root")?;
        let l2 = dump(live.engine(), root);
        let (f1, f2, threw) = look(&main, a.size)?;
        // What a load leaves to chance (`Math.random()`) differs between
        // two of them.
        let (g1, _, _) = look(&main, a.size)?;
        let _ = log.take();
        // The fresh loads are later than the second look at the running
        // application: what a third look agrees with them on changed in
        // between (a minute turning over).
        let root = live.root().context("root")?;
        let l3 = dump(live.engine(), root);
        let mut skip = moving(&l1, &l2);
        skip.extend(moving(&f1, &f2));
        skip.extend(moving(&f1, &g1));
        skip.extend(
            l3.iter()
                .filter(|&(k, v)| f2.get(k) == Some(v))
                .map(|(k, _)| k.clone()),
        );
        let all = differences(&l2, &f2, &skip);
        let (kept, diff): (Vec<String>, Vec<String>) = all.into_iter().partition(|d| {
            let Some((k, _)) = d.split_once(": live ") else {
                return false;
            };
            // Animations go on through a reload and a Behavior is not
            // played for a value put back: where one has written, the
            // run's history shows.
            let animated = |m: &Dump| m.get(k).is_some_and(|v| v.starts_with('~'));
            if animated(&l2) || animated(&f2) || k.contains("/Behavior[") {
                return true;
            }
            let carried = l2.get(k).is_some_and(|v| was_live.get(k) == Some(v));
            // A difference that was there before the edit, on both sides as
            // it was, is not the edit's: what follows from state kept by an
            // earlier one (a colour bound to a value a script wrote).
            if carried && f2.get(k) == was_fresh.get(k) && was_live.get(k) != was_fresh.get(k) {
                return true;
            }
            let written = l2.get(k).is_some_and(|v| v.starts_with('='))
                && f2.get(k).is_some_and(|v| v.starts_with('='));
            // Built up while it ran, or written by a script that the
            // edited files now stop short of on a fresh load.
            carried && written && (was_live.get(k) != was_fresh.get(k) || (threw && !was_threw))
        });
        if !kept.is_empty() {
            *counts.entry("kept state").or_default() += 1;
        }
        // A window's size the file gives is put in place by a reload; the
        // host sizes a fresh load's window, so what follows that size
        // cannot be judged here.
        let root_key = |m: &Dump, p: &str| {
            m.keys()
                .find(|k| !k.contains('/') && k.ends_with(p))
                .and_then(|k| m.get(k))
                .cloned()
        };
        let host_sized = [" .width", " .height"]
            .iter()
            .any(|p| root_key(&l2, p) != root_key(&f2, p));
        let (kept, diff) = if host_sized && !diff.is_empty() {
            *counts.entry("window sized by the host").or_default() += 1;
            let mut kept = kept;
            kept.push(format!(
                "{} differences follow the window's size, which the host gives a fresh load",
                diff.len()
            ));
            (kept, Vec::new())
        } else {
            (kept, diff)
        };
        writeln!(
            stdout,
            "#{n} {file}: {what}: {} ({} kept, {} new){}{}",
            if full { "full path" } else { "in place" },
            outcome.kept,
            outcome.fresh,
            if full {
                format!(": {}", why.first().map_or("?", |w| w.as_str()))
            } else {
                String::new()
            },
            if kept.is_empty() {
                String::new()
            } else {
                format!("; {} values the run built up kept", kept.len())
            },
        )?;
        for d in kept.iter().take(4) {
            writeln!(stdout, "    kept {d}")?;
        }
        if a.stop_at == Some(n) {
            for d in kept.iter().chain(&diff) {
                writeln!(stdout, "    {d}")?;
            }
            return Ok(());
        }
        if !diff.is_empty() {
            *counts.entry("diverged").or_default() += 1;
            writeln!(stdout, "  DIVERGED in {} places:", diff.len())?;
            for d in diff.iter().take(12) {
                writeln!(stdout, "    {d}")?;
            }
            // Start again from the files, so the next edit is judged on
            // its own.
            drop(live);
            live = App::open(&main, a.size)?;
            live.settle(Duration::from_millis(1500));
            let _ = log.take();
            was_live = live
                .root()
                .map(|r| dump(live.engine(), r))
                .unwrap_or_default();
        } else {
            was_live = l2;
        }
        was_fresh = f2;
        was_threw = threw;
        stdout.flush()?;
    }
    for (p, text) in &originals {
        std::fs::write(p, text)?;
    }
    writeln!(stdout, "== {} edits: {counts:?}", n)?;
    writeln!(stdout, "== full path by kind: {kinds_full:?}")?;
    writeln!(stdout, "== full path by reason: {reasons:?}")?;
    Ok(())
}
