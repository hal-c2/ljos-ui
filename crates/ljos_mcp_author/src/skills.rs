//! The prompts as Agent Skills: `skills/<name>/SKILL.md` at the repository
//! root, and beside it in `references/` the documents a prompt points to,
//! since a user's project has no `docs/` of Ljós's. The prompts are the
//! source; the files are written from them, and a test fails when what is
//! committed differs from what would be written. `LJOS_SKILLS=write cargo
//! test -p ljos_mcp_author` writes them.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::prompts::{self, Reader};

/// A prompt as a skill: what the skill is called, what it says of itself
/// to an agent choosing one, and what the work is on, said without the
/// prompt's arguments, which an agent with a skill learns from its user.
struct Skill {
    prompt: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    intro: &'static str,
}

const SKILLS: &[Skill] = &[
    Skill {
        prompt: "write-component",
        name: "writing-qml-components",
        title: "Writing QML components",
        description: "Writes a new QML component or screen for a Ljós application, and checks it \
            until the compiler, the lint and the design audit report no Error. Use when the user \
            asks to add, build or design a QML component, view, panel, dialog or screen for Ljós.",
        intro: "Write the Ljós QML component the user asks for, for the purpose they give.",
    },
    Skill {
        prompt: "port-from-qtquick",
        name: "porting-from-qt-quick",
        title: "Porting from Qt Quick",
        description: "Ports QML written for Qt Quick so that it runs on Ljós, keeping what it \
            draws and does, and says what could not be ported. Use when the user has Qt QML to \
            port, migrate or run on Ljós, or when Ljós rejects QML that works on Qt.",
        intro: "Port the Qt Quick QML the user points to, files in the project or QML they \
            paste, to Ljós.",
    },
    Skill {
        prompt: "add-accessibility",
        name: "adding-accessibility",
        title: "Adding accessibility",
        description: "Gives every control in Ljós QML a name and a role a screen reader can say, \
            and holds that fixed with a scenario. Use when the user asks to make QML accessible \
            or usable with a screen reader, or to fix a11y lint findings.",
        intro: "Make the QML files or directories the user names usable by someone who cannot \
            see them.",
    },
    Skill {
        prompt: "design-review",
        name: "reviewing-qml-design",
        title: "Reviewing QML design",
        description: "Reviews Ljós QML for UX, visual accessibility and motion problems, such as \
            low contrast, targets too small to press, text cut off or controls off the window, \
            and fixes them. Use when the user asks for a design review or a UI audit of QML, or \
            to fix what `ljos audit` or the design lints report.",
        intro: "Review the QML file, directory or text the user gives for UX, visual \
            accessibility and motion problems, and fix them.",
    },
    Skill {
        prompt: "localise",
        name: "localising-qml",
        title: "Localising QML",
        description: "Makes every text a user reads in Ljós QML translatable with qsTr, and \
            brings the translation catalogues up to date. Use when the user asks to translate, \
            localise or internationalise QML, or to fix i18n lint findings.",
        intro: "Make every text a user reads in the QML files or directories the user names \
            translatable.",
    },
    Skill {
        prompt: "write-feature",
        name: "writing-scenarios",
        title: "Writing scenarios",
        description: "Describes one behaviour of Ljós or of a QML type as a Gherkin scenario \
            the Ljós runner can run, before or beside the code that has it. Use when the user \
            asks for a scenario or a feature file for how a type or property behaves.",
        intro: "Describe the behaviour the user states as a scenario the runner can run.",
    },
    Skill {
        prompt: "test-app",
        name: "testing-qml-apps",
        title: "Testing QML apps",
        description: "Specifies what a QML application does as Gherkin scenarios that run on \
            Ljós and on Qt, and runs them on both. Use when the user asks to test a QML app or \
            component, to write behaviour tests or feature files for their QML, or to check that \
            it behaves the same on Ljós and on Qt.",
        intro: "Specify what the user's QML files do as scenarios that run on Ljós and on Qt, \
            and run them.",
    },
    Skill {
        prompt: "embed-in-rust",
        name: "embedding-qml-in-rust",
        title: "Embedding QML in Rust",
        description: "Writes a Rust program that shows a QML interface with Ljós: the App, \
            native types, list models from Rust, threads that update the interface through \
            UiHandle, and a headless test. Use when the user asks to embed or drive QML from \
            Rust, or to build a Rust application with a QML interface.",
        intro: "Write the Rust program the user describes, which shows its interface in QML \
            with Ljós. It needs Rust and Cargo besides what every skill here needs.",
    },
];

/// What an agent needs before a skill's steps can be taken.
const REQUIRES: &str = "## Requires\n\n\
    - The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).\n\
    - Its authoring server connected to the agent as the MCP server `ljos`: \
    `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.\n";

/// What a skill that runs scenarios needs as well.
const RUNS: &str = "- `ljos:features_run` needs the server started with `--caps run` and \
    `--features-runner <program>`, the program that runs scenarios.\n";

/// The documents a prompt's text points to, in the order it names them.
/// A pattern such as `docs/design/<page>` names no one document.
fn docs_named(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("docs/") {
        let rest = &text[at..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || "/_.-".contains(c)))
            .unwrap_or(rest.len());
        let path = rest[..end].trim_end_matches('.');
        if path.ends_with(".md") && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// A word in backticks that is a tool becomes `ljos:<tool>`: an agent with
/// a skill may have tools of other servers by the same name.
fn qualify(text: &str) -> String {
    text.split('`')
        .enumerate()
        .map(|(index, word)| {
            if index % 2 == 1 && !prompts::tools_named(&format!("`{word}`")).is_empty() {
                format!("ljos:{word}")
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("`")
}

/// A document as a reference: without the site's front matter, and with
/// its headings listed first when it is long enough to be read in parts.
fn reference(source: &str) -> String {
    let body = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(source, |(_, body)| body)
        .trim_start();
    if body.lines().count() <= 100 {
        return body.to_owned();
    }
    let mut fenced = false;
    let mut contents = String::from("## Contents\n\n");
    for line in body.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if !fenced && let Some(heading) = line.strip_prefix("## ") {
            contents.push_str(&format!("- {heading}\n"));
        }
    }
    match body.split_once('\n') {
        Some((title, rest)) if title.starts_with("# ") => {
            format!("{title}\n\n{contents}\n{}", rest.trim_start())
        }
        _ => format!("{contents}\n{body}"),
    }
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file of every skill, by its path under `skills/`.
fn files() -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    for skill in SKILLS {
        let body = prompts::body(skill.prompt, Reader::Skill).expect("a skill's prompt exists");
        let description = skill
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        // Quoted, since a plain YAML scalar cannot hold ": ".
        let description = serde_json::to_string(&description).unwrap();
        let runs = if body.contains("`features_run`") {
            RUNS
        } else {
            ""
        };
        let text = format!(
            "---\nname: {}\ndescription: {description}\n---\n\n# {}\n\n{}\n\n{REQUIRES}{runs}\n## Steps\n\n{}\n",
            skill.name,
            skill.title,
            skill.intro.split_whitespace().collect::<Vec<_>>().join(" "),
            qualify(&body),
        );
        let dir = PathBuf::from(skill.name);
        out.insert(dir.join("SKILL.md"), text);
        let server = prompts::body(skill.prompt, Reader::Server).expect("a skill's prompt exists");
        // What the server's text quotes of a document, such as the brief's
        // own pointers, is not something the skill points to.
        for doc in docs_named(&server) {
            let name = prompts::doc(Reader::Skill, doc);
            if !body.contains(&name) {
                continue;
            }
            let source = std::fs::read_to_string(repository().join(doc))
                .unwrap_or_else(|e| panic!("{doc}: {e}"));
            out.insert(dir.join(name), reference(&source));
        }
    }
    out
}

#[test]
fn the_skills_are_written_from_the_prompts() {
    let root = repository().join("skills");
    let files = files();
    let write = std::env::var("LJOS_SKILLS").is_ok_and(|v| v == "write");
    // A skill whose prompt is gone is a directory with a SKILL.md that
    // nothing writes any more.
    let stale: Vec<PathBuf> = std::fs::read_dir(&root)
        .map(|dir| {
            dir.filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.join("SKILL.md").is_file())
                .filter(|path| !SKILLS.iter().any(|s| path.ends_with(s.name)))
                .collect()
        })
        .unwrap_or_default();
    if write {
        for path in &stale {
            std::fs::remove_dir_all(path).unwrap();
        }
        for skill in SKILLS {
            let _ = std::fs::remove_dir_all(root.join(skill.name));
        }
        for (path, text) in &files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        return;
    }
    assert!(
        stale.is_empty(),
        "skills no prompt writes: {stale:?}; LJOS_SKILLS=write deletes them"
    );
    for (path, text) in &files {
        let committed = std::fs::read_to_string(root.join(path)).unwrap_or_default();
        assert!(
            committed == *text,
            "skills/{} is not what the prompts write; LJOS_SKILLS=write cargo test -p ljos_mcp_author writes it",
            path.display()
        );
    }
    for skill in SKILLS {
        let mut on_disk = Vec::new();
        for entry in walk(&root.join(skill.name)) {
            on_disk.push(entry.strip_prefix(&root).unwrap().to_path_buf());
        }
        on_disk.retain(|p| !files.contains_key(p));
        assert!(on_disk.is_empty(), "files no prompt writes: {on_disk:?}");
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// The rules of the Agent Skills format a skill is held to.
#[test]
fn every_skill_keeps_the_format() {
    for skill in SKILLS {
        assert!(skill.name.len() <= 64, "{}", skill.name);
        assert!(
            skill
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-'),
            "{}",
            skill.name
        );
        let first = skill.name.split('-').next().unwrap();
        assert!(first.ends_with("ing"), "{} is not a gerund", skill.name);
        assert!(skill.description.len() <= 1024, "{}", skill.name);
        assert!(skill.description.contains(" Use when "), "{}", skill.name);
        assert!(!skill.description.contains('<'), "{}", skill.name);
    }
    for (path, text) in files() {
        if path.ends_with("SKILL.md") {
            assert!(text.lines().count() < 500, "{}", path.display());
        } else {
            // A reference read in parts says what parts it has.
            assert!(
                text.lines().count() <= 100 || text.contains("## Contents"),
                "{}",
                path.display()
            );
        }
    }
    let prompts: Vec<&str> = prompts::list().iter().map(|p| p.name).collect();
    let skilled: Vec<&str> = SKILLS.iter().map(|s| s.prompt).collect();
    assert_eq!(prompts, skilled, "every prompt is a skill");
}
