//! The examples of docs/site, as ljos.dev shows each one running beside its
//! code (docs/site/README.md): every fence says what it is, every QML
//! example passes `ljos check`, and every one but the porting page's
//! "before" halves is written in Ljós's own API, which `ljos migrate` has
//! nothing left to lower in.
//!
//! What is not there yet is listed in `docs_site.txt` beside this file, with
//! why; an entry that no longer applies fails too, so the list only shrinks.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

/// The tags of blocks that never run. A `javascript` block is host-side
/// script unless it names a `group`, where it is a file of that example.
const EXEMPT: &[&str] = &[
    "sh",
    "rust",
    "toml",
    "json",
    "text",
    "xml",
    "gherkin",
    "javascript",
    "diff",
];

/// The page whose `qml before` blocks show Qt's QML next to what
/// `ljos migrate` makes of it.
const PORTING: &str = "language/porting-from-qt.md";

/// What the harness a `harness=` snippet goes into has in place of it.
const SNIPPET: &str = "//! snippet";

/// A `ljos check` past this is ended and reported rather than waited for:
/// it once hung on a TypeScript `Tap` handler (#394).
const DEADLINE: Duration = Duration::from_secs(60);

fn site() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/site")
}

struct Block {
    page: String,
    line: usize,
    tag: String,
    attrs: BTreeMap<String, String>,
    flags: BTreeSet<String>,
    body: String,
    /// Which QML block of its page this is, from 1.
    nth: usize,
}

impl Block {
    fn at(&self) -> String {
        format!("docs/site/{}:{}", self.page, self.line)
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}

fn pages(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            pages(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

fn fence(line: &str) -> Option<(usize, usize, &str)> {
    let rest = line.trim_start();
    let indent = line.len() - rest.len();
    let ticks = rest.bytes().take_while(|&b| b == b'`').count();
    (ticks >= 3).then(|| (indent, ticks, rest[ticks..].trim()))
}

fn blocks(page: &str, text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut nth = 0;
    let mut i = 0;
    while i < lines.len() {
        let Some((indent, ticks, info)) = fence(lines[i]) else {
            i += 1;
            continue;
        };
        let end = (i + 1..lines.len())
            .find(|&j| fence(lines[j]).is_some_and(|(_, t, rest)| t >= ticks && rest.is_empty()))
            .unwrap_or_else(|| panic!("docs/site/{page}:{}: a fence that is never closed", i + 1));
        let mut words = info.split_whitespace();
        let tag = words.next().unwrap_or_default().to_owned();
        let (mut attrs, mut flags) = (BTreeMap::new(), BTreeSet::new());
        for word in words {
            match word.split_once('=') {
                Some((key, value)) => {
                    attrs.insert(key.to_owned(), value.to_owned());
                }
                None => {
                    flags.insert(word.to_owned());
                }
            }
        }
        let mut body = String::new();
        for line in &lines[i + 1..end] {
            let cut = line.len() - line.trim_start().len();
            body.push_str(&line[cut.min(indent)..]);
            body.push('\n');
        }
        if tag == "qml" {
            nth += 1;
        }
        out.push(Block {
            page: page.to_owned(),
            line: i + 1,
            tag,
            attrs,
            flags,
            body,
            nth,
        });
        i = end + 1;
    }
    out
}

/// One example as `ljos check` reads it: a directory of files.
struct Project {
    /// `<page>#<n>`: the page and which of its QML blocks is the root.
    key: String,
    at: String,
    files: Vec<(String, String)>,
    before: bool,
    /// For a `before`, the key of the block after it, which is what
    /// `ljos migrate` makes of it.
    after: Option<String>,
}

fn harnessed(block: &Block, problems: &mut Vec<String>) -> String {
    let Some(name) = block.attr("harness") else {
        return block.body.clone();
    };
    let path = site().join("harness").join(format!("{name}.qml"));
    let Ok(harness) = fs::read_to_string(&path) else {
        problems.push(format!(
            "{}: no harness docs/site/harness/{name}.qml",
            block.at()
        ));
        return block.body.clone();
    };
    let mut out = String::new();
    for line in harness.lines() {
        if line.trim() == SNIPPET {
            let indent = &line[..line.len() - line.trim_start().len()];
            for snippet in block.body.lines() {
                if !snippet.is_empty() {
                    out.push_str(indent);
                    out.push_str(snippet);
                }
                out.push('\n');
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn projects(all: &[Block], problems: &mut Vec<String>) -> Vec<Project> {
    let mut out = Vec::new();
    let mut groups: BTreeMap<(String, String), Vec<&Block>> = BTreeMap::new();
    for (i, block) in all.iter().enumerate() {
        let tag = block.tag.as_str();
        if tag.is_empty() {
            problems.push(format!(
                "{}: a bare fence; say what the block is (docs/site/README.md)",
                block.at()
            ));
            continue;
        }
        let grouped = block.attr("group").map(str::to_owned);
        match tag {
            "qml" => {}
            "ts" | "typescript" => {
                if grouped.is_none() || block.attr("file").is_none() {
                    problems.push(format!(
                        "{}: a `{tag}` block is a file of an example: give it `file=` and `group=`",
                        block.at()
                    ));
                    continue;
                }
            }
            "javascript" if grouped.is_some() => {
                if block.attr("file").is_none() {
                    problems.push(format!(
                        "{}: a grouped `javascript` block needs `file=`",
                        block.at()
                    ));
                    continue;
                }
            }
            _ if EXEMPT.contains(&tag) => continue,
            _ => {
                problems.push(format!(
                    "{}: `{tag}` is not a tag the docs know (docs/site/README.md)",
                    block.at()
                ));
                continue;
            }
        }
        let before = block.flags.contains("before");
        if before && block.page != PORTING {
            problems.push(format!(
                "{}: `qml before` belongs on {PORTING} only",
                block.at()
            ));
        }
        if let Some(unknown) = block.flags.iter().find(|flag| *flag != "before") {
            problems.push(format!(
                "{}: `{unknown}` is not a flag the docs know",
                block.at()
            ));
        }
        if let Some(group) = grouped {
            groups
                .entry((block.page.clone(), group))
                .or_default()
                .push(block);
            continue;
        }
        if tag != "qml" {
            continue;
        }
        let after = before.then(|| {
            all[i + 1..]
                .iter()
                .take_while(|next| next.page == block.page)
                .find(|next| next.tag == "qml")
                .map(|next| format!("{}#{}", next.page, next.nth))
        });
        if after == Some(None) {
            problems.push(format!(
                "{}: a `qml before` with no `qml` block after it",
                block.at()
            ));
        }
        out.push(Project {
            key: format!("{}#{}", block.page, block.nth),
            at: block.at(),
            files: vec![("Main.qml".to_owned(), harnessed(block, problems))],
            before,
            after: after.flatten(),
        });
    }
    for ((page, group), members) in groups {
        let roots: Vec<_> = members
            .iter()
            .filter(|block| block.tag == "qml" && block.attr("file").is_none())
            .collect();
        let [root] = roots[..] else {
            problems.push(format!(
                "docs/site/{page}: group `{group}` has {} root blocks (a `qml` block without `file=`); it needs one",
                roots.len()
            ));
            continue;
        };
        let mut files: Vec<(String, String)> = Vec::new();
        for block in &members {
            if block.flags.contains("before") {
                problems.push(format!(
                    "{}: a `qml before` in group `{group}`: only an ungrouped one is compared with the block after it",
                    block.at()
                ));
            }
            let name = block.attr("file").unwrap_or("Main.qml").to_owned();
            if files.iter().any(|(seen, _)| *seen == name) {
                problems.push(format!(
                    "{}: group `{group}` has two files named {name}",
                    block.at()
                ));
            }
            files.push((name, harnessed(block, problems)));
        }
        out.push(Project {
            key: format!("{page}#{}", root.nth),
            at: root.at(),
            files,
            before: members.iter().any(|block| block.flags.contains("before")),
            after: None,
        });
    }
    out
}

/// What each `qml before` should migrate to: the files of the example
/// after it, by the before's key. One whose after is no example's root, or
/// is another `before`, is a problem rather than a comparison skipped.
fn afters(
    projects: &[Project],
    problems: &mut Vec<String>,
) -> BTreeMap<String, Vec<(String, String)>> {
    let by_key: BTreeMap<_, _> = projects.iter().map(|p| (p.key.as_str(), p)).collect();
    let mut out = BTreeMap::new();
    for project in projects {
        let Some(key) = &project.after else {
            continue;
        };
        match by_key.get(key.as_str()) {
            None => problems.push(format!(
                "{} ({}): the `qml` block after it ({key}) is a file of a group, not an example's root, so nothing compares them; put the after half right after it, ungrouped or as its group's root",
                project.at, project.key
            )),
            Some(after) if after.before => problems.push(format!(
                "{} ({}): the `qml` block after it ({key}) is another `qml before`",
                project.at, project.key
            )),
            Some(after) => {
                out.insert(project.key.clone(), after.files.clone());
            }
        }
    }
    out
}

/// Why an example is not yet what the docs promise, from `docs_site.txt`.
#[derive(Default)]
struct Pending {
    /// Not in Ljós's API yet: the migrate gap that keeps it in Qt's.
    gap: Option<String>,
    /// It uses a type a Rust program registers, which `ljos check` has no
    /// description of.
    host_types: bool,
    /// It shows what the checks find, so `ljos check` must fail on it.
    shows_findings: bool,
}

fn pending() -> BTreeMap<String, Pending> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/docs_site.txt");
    let mut out = BTreeMap::new();
    for (n, line) in fs::read_to_string(path).unwrap().lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default();
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // The page and block are written `<page> <n>`: `#` starts a comment.
        let mut words = line.split_whitespace();
        let (Some(page), Some(nth)) = (words.next(), words.next()) else {
            panic!("tests/docs_site.txt:{}: `<page> <n> <reason>…`", n + 1);
        };
        let mut entry = Pending::default();
        for reason in words {
            match reason {
                "host-types" => entry.host_types = true,
                "shows-findings" => entry.shows_findings = true,
                gap if gap.starts_with("gap:") => entry.gap = Some(gap[4..].to_owned()),
                other => panic!("tests/docs_site.txt:{}: no reason `{other}`", n + 1),
            }
        }
        out.insert(format!("{page}#{nth}"), entry);
    }
    out
}

/// Why a `host-types` entry no longer applies, from how `ljos check` ended:
/// it should fail, and only because it cannot find the program's types.
fn stale_host_types(passed: bool, out: &str) -> Option<String> {
    if passed {
        return Some("`ljos check` passes it".to_owned());
    }
    let errors: Vec<_> = out
        .lines()
        .filter(|line| line.contains(": error: "))
        .collect();
    if errors.is_empty() {
        return Some(format!("`ljos check` fails with no error found:\n{out}"));
    }
    let other: Vec<_> = errors
        .into_iter()
        .filter(|line| !line.trim_end().ends_with("[unknown-type]"))
        .collect();
    (!other.is_empty()).then(|| {
        format!(
            "`ljos check` fails on more than the program's types:\n{}",
            other.join("\n")
        )
    })
}

/// `ljos` in `dir`, ended at the deadline: `None` when it was.
fn ljos(dir: &Path, args: &[&str]) -> Option<(bool, String)> {
    let log = dir.with_extension("log");
    // One handle for both streams: two would each keep their own offset
    // and write over each other.
    let out = fs::File::create(&log).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(out.try_clone().unwrap())
        .stdout(out)
        .spawn()
        .expect("ljos runs");
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    Some((
        status.success(),
        fs::read_to_string(&log).unwrap_or_default(),
    ))
}

fn write(dir: &Path, files: &[(String, String)]) {
    for (name, text) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

/// Qt's modules, which a file in Ljós's API does not import.
fn imports_qt(files: &[(String, String)]) -> Option<String> {
    files
        .iter()
        .filter(|(name, _)| name.ends_with(".qml"))
        .flat_map(|(_, text)| text.lines())
        .flat_map(|line| line.split(';'))
        .map(str::trim)
        .find(|line| {
            line.strip_prefix("import ")
                .is_some_and(|module| module.trim_start().starts_with("Qt"))
        })
        .map(str::to_owned)
}

/// What is wrong with one example, if anything.
fn verify(
    root: &Path,
    project: &Project,
    pending: Option<&Pending>,
    after: Option<&Vec<(String, String)>>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let id = project.key.replace(['/', '#', '.'], "_");
    let dir = root.join(&id);
    write(&dir, &project.files);
    let host_types = pending.is_some_and(|p| p.host_types);
    let shows_findings = pending.is_some_and(|p| p.shows_findings);
    match ljos(&dir, &["check", "."]) {
        None => problems.push(format!(
            "{} ({}): `ljos check` ran past {}s (see #394)",
            project.at,
            project.key,
            DEADLINE.as_secs()
        )),
        Some((passed, out)) if host_types => {
            if let Some(why) = stale_host_types(passed, &out) {
                problems.push(format!(
                    "{} ({}): listed as using a Rust program's types, but {why}; take `host-types` off tests/docs_site.txt or fix the block",
                    project.at, project.key
                ));
            }
        }
        Some((true, _)) if shows_findings => problems.push(format!(
            "{} ({}): listed as showing findings, but `ljos check` passes it",
            project.at, project.key
        )),
        Some((false, out)) if !shows_findings => problems.push(format!(
            "{} ({}): `ljos check` fails:\n{out}",
            project.at, project.key
        )),
        Some(_) => {}
    }
    if !project.before {
        let qt = imports_qt(&project.files);
        let lowers = match ljos(&dir, &["migrate", "."]) {
            Some((_, out)) => (!out.contains(" 0 lowered,")).then_some(out),
            None => Some("`ljos migrate` ran past the deadline".to_owned()),
        };
        let dialect = qt.is_none() && lowers.is_none();
        match (pending.and_then(|p| p.gap.as_deref()), dialect) {
            (Some(gap), true) => problems.push(format!(
                "{} ({}): now in Ljós's API; take it off tests/docs_site.txt (gap {gap})",
                project.at, project.key
            )),
            (None, false) => problems.push(format!(
                "{} ({}): not in Ljós's API: {}; write it as `ljos migrate --write` does, or list the migrate gap in tests/docs_site.txt",
                project.at,
                project.key,
                qt.map(|line| format!("`{line}`"))
                    .or(lowers)
                    .unwrap_or_default()
            )),
            _ => {}
        }
    }
    if let Some(after) = after {
        let moved = root.join(format!("{id}_migrated"));
        write(&moved, &project.files);
        let Some((true, out)) = ljos(&moved, &["migrate", "--write", "."]) else {
            problems.push(format!(
                "{} ({}): `ljos migrate --write` fails",
                project.at, project.key
            ));
            return problems;
        };
        let read = |name: &str| fs::read_to_string(moved.join(name)).unwrap_or_default();
        let lowered: Vec<_> = after.iter().map(|(name, _)| read(name)).collect();
        let _ = ljos(&moved, &["fmt", "."]);
        let formatted: Vec<_> = after.iter().map(|(name, _)| read(name)).collect();
        let shown: Vec<_> = after.iter().map(|(_, text)| text.clone()).collect();
        if shown != lowered && shown != formatted {
            problems.push(format!(
                "{} ({}): the block after it is not what `ljos migrate --write` makes of it ({out}):\n{}",
                project.at,
                project.key,
                lowered.join("\n")
            ));
        }
    }
    problems
}

#[test]
fn every_example_says_what_it_is_and_qml_passes_ljos_check_in_ljos_api() {
    let mut paths = Vec::new();
    pages(&site(), &mut paths);
    let mut all = Vec::new();
    for path in &paths {
        let page = path
            .strip_prefix(site())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if page.starts_with("harness/") {
            continue;
        }
        all.extend(blocks(&page, &fs::read_to_string(path).unwrap()));
    }
    let mut problems = Vec::new();
    let mut projects = projects(&all, &mut problems);

    // The harnesses are examples of their own: one snippet line, Ljós's API.
    let harnesses = site().join("harness");
    if harnesses.is_dir() {
        for entry in fs::read_dir(&harnesses).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = fs::read_to_string(&path).unwrap();
            let marks = text.lines().filter(|line| line.trim() == SNIPPET).count();
            if marks != 1 {
                problems.push(format!(
                    "docs/site/harness/{name}: {marks} `{SNIPPET}` lines; a harness has one"
                ));
            }
            projects.push(Project {
                key: format!("harness/{name}#0"),
                at: format!("docs/site/harness/{name}"),
                files: vec![("Main.qml".to_owned(), text)],
                before: false,
                after: None,
            });
        }
    }

    let mut pending = pending();
    let known: BTreeSet<_> = projects.iter().map(|p| p.key.clone()).collect();
    for key in pending.keys() {
        if !known.contains(key) {
            problems.push(format!(
                "tests/docs_site.txt: `{}` is no example's root block",
                key.replace('#', " ")
            ));
        }
    }
    let afters = afters(&projects, &mut problems);

    let root = tempfile::tempdir().unwrap();
    let queue = Mutex::new(projects.iter());
    let found = Mutex::new(problems);
    let workers = thread::available_parallelism().map_or(4, |n| n.get().min(8));
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some(project) = queue.lock().unwrap().next() else {
                        break;
                    };
                    let mine = verify(
                        root.path(),
                        project,
                        pending.get(&project.key),
                        afters.get(&project.key),
                    );
                    found.lock().unwrap().extend(mine);
                }
            });
        }
    });
    pending.clear();
    let problems = found.into_inner().unwrap();
    assert!(
        problems.is_empty(),
        "{} problems with the docs/site examples:\n\n{}",
        problems.len(),
        problems.join("\n\n")
    );
}

/// The problems with one page of fences, as the docs test reads them.
fn page_problems(page: &str, text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let projects = projects(&blocks(page, text), &mut problems);
    afters(&projects, &mut problems);
    problems
}

#[test]
fn a_before_whose_after_is_no_examples_root_is_a_problem() {
    let file_of_a_group = "```qml before\nItem {}\n```\n\n\
        ```qml group=g file=Card.qml\nItem {}\n```\n\n\
        ```qml group=g\nCard {}\n```\n";
    let problems = page_problems(PORTING, file_of_a_group);
    assert!(
        problems.iter().any(
            |p| p.starts_with(&format!("docs/site/{PORTING}:1 ({PORTING}#1)"))
                && p.contains(&format!("({PORTING}#2) is a file of a group"))
        ),
        "{problems:#?}"
    );

    let another_before =
        "```qml before\nItem {}\n```\n\n```qml before\nItem {}\n```\n\n```qml\nItem {}\n```\n";
    let problems = page_problems(PORTING, another_before);
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with(&format!("docs/site/{PORTING}:1 "))
                && p.contains("is another `qml before`")),
        "{problems:#?}"
    );

    let grouped_before = "```qml before group=g\nItem {}\n```\n\n```qml\nItem {}\n```\n";
    let problems = page_problems(PORTING, grouped_before);
    assert!(
        problems.iter().any(|p| p.starts_with(&format!(
            "docs/site/{PORTING}:1: a `qml before` in group `g`"
        ))),
        "{problems:#?}"
    );

    let compared = "```qml before\nItem {}\n```\n\n```qml\nItem {}\n```\n";
    assert_eq!(page_problems(PORTING, compared), Vec::<String>::new());
}

#[test]
fn a_host_types_entry_is_stale_once_check_passes_or_fails_on_anything_else() {
    let unknown = "Main.qml:8:5: error: `Counter` is not a type of anything the file imports [unknown-type]\n\
        Main.qml:22:9: warning: `onClicked` on a `MouseArea` [focus-pointer-only]\n\
        1 files, 1 errors, 1 warnings\nljos: 1 errors\n";
    assert_eq!(stale_host_types(false, unknown), None);

    let passes = stale_host_types(true, "1 files, 0 errors, 0 warnings\n");
    assert!(passes.is_some_and(|why| why.contains("passes")));

    let other = format!(
        "{unknown}Main.qml:3:1: error: `wdth` is not a property of `Item` [unknown-property]\n"
    );
    let why = stale_host_types(false, &other).expect("another error makes it stale");
    assert!(
        why.contains("[unknown-property]") && !why.contains("[unknown-type]"),
        "{why}"
    );

    assert!(stale_host_types(false, "ljos: cannot read .\n").is_some());
}
