//! Ways of working an agent can ask for by name. Each says which tools to
//! call and in what order. The same texts are the skills in `skills/` at the
//! repository root (see `skills.rs`): a prompt is read by an agent that
//! asked the server for it, with the arguments it gave; a skill by an agent
//! whose user names what the work is on in the conversation.
//!
//! A word in backticks with an underscore in it is the name of a tool, and
//! of nothing else: a test holds every such word against the tools the
//! server has, so a prompt cannot send an agent to a tool that is not there.

use ljos_mcp_core::{PromptArg, PromptDef, PromptText, SurfaceError};
use serde_json::{Map, Value};

/// The brief the design prompts carry, as the document has it, so that
/// what an agent is told and what `docs/design/` says cannot drift apart.
const AGENT_BRIEF: &str = include_str!("../../../docs/design/agent-brief.md");

/// Who reads a prompt's text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reader {
    /// An agent that asked this server, which reads the project's `docs/`.
    Server,
    /// An agent with the skill, whose pages are beside it in `references/`:
    /// a user's project has no `docs/` of Ljós's.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the skills are written by a test")
    )]
    Skill,
}

/// Where a document is for `reader`.
pub(crate) fn doc(reader: Reader, path: &str) -> String {
    match reader {
        Reader::Server => path.to_owned(),
        Reader::Skill => format!("references/{}", path.rsplit('/').next().unwrap_or(path)),
    }
}

/// The loop that ends every prompt that writes QML. The audit and `ljos
/// check` are the command line's, not this server's, so they are written
/// as commands: a word in backticks with an underscore must be a tool here.
const VALIDATE: &str = "Then check, and repeat until none of these reports an Error:\n   \
    - `ljos check <dir>` in a terminal: the compiler and the static lints, for every file in the \
    directory.\n   \
    - `qml_lint` on every file you touched.\n   \
    - `ljos audit <file.qml>` in a terminal, on the file that opens the window (on an application \
    started with `ljos run --mcp`, the inspector's tool app_audit with `matrix` set to `full`). It \
    draws the scene in light and dark, at high contrast, at 200 % text and in the small window, \
    and finds what the lint cannot see: a target too small to press, a control off the window or \
    covered, text cut off, contrast over what is behind it.\n\n   \
    Fix every Error. Fix every Warning, or keep it with `// ljos-allow(<code>): <reason>` where \
    the finding is. The work is not done while any of the three reports an Error.";

/// Said at the end of every prompt: an answer an agent must not read past.
const UNAVAILABLE: &str = "A tool error with the code `unavailable`, or one that says a capability \
    is not granted, means that check did not happen: say so in your answer, do not call the work \
    checked.";

/// How a scenario is written for the runner, which a model cannot guess.
const SCENARIO_RULES: &str = "How a scenario is written here:\n   \
    - Use only the steps the resource `ljos://features/steps` lists, word for word: any other \
    wording fails with \"Step doesn't match any function\". Pointer steps by position (`the user \
    presses the pointer at X, Y`, then `releases`) run on Ljós and on Qt; finding a control by \
    its role and name needs Ljós's accessibility bridge and fails on Qt.\n   \
    - A scenario sees only the files it declares. Give each with `Given the file \"X.qml\":` and \
    a doc string labelled `qml`, at most 25 lines, and load it with `When \"X.qml\" is loaded`: \
    after a Given, an And is a Given too.\n   \
    - Objects are found by `objectName`, not by `id`: give the objects you check one in the \
    scenario's copy of the file.\n   \
    - The file starts with a `# Sources:` comment block, one source per line (the files it \
    describes, Qt documentation URLs). A file tagged `@qt-parity` then has the line \
    `# Verified on Qt 6.x.y (qml6, offscreen).` with the Qt version it passed on.";

pub(crate) fn list() -> Vec<PromptDef> {
    let paths = |what: &'static str| PromptArg {
        name: "paths",
        description: what,
        required: true,
    };
    vec![
        PromptDef {
            name: "write-component",
            description: "Write a new QML component: the ledger, the design rules, the QML, then format, check, lint, tiers and the audit.",
            arguments: vec![
                PromptArg {
                    name: "name",
                    description: "Component name, for example `NoteCard`.",
                    required: true,
                },
                PromptArg {
                    name: "purpose",
                    description: "What the component is for.",
                    required: false,
                },
            ],
        },
        PromptDef {
            name: "port-from-qtquick",
            description: "Port Qt Quick QML to Ljós: what the checks reject, what the ledger says of it, the replacements, then the checks again.",
            arguments: vec![PromptArg {
                name: "source",
                description: "The QML to port, or a path inside the project.",
                required: true,
            }],
        },
        PromptDef {
            name: "add-accessibility",
            description: "Give every item a user acts on a name and a role a screen reader can say, and hold that fixed with a scenario.",
            arguments: vec![paths(
                "The `.qml` files or directories to go through, for example `views`.",
            )],
        },
        PromptDef {
            name: "design-review",
            description: "Review QML for UX, visual accessibility and motion problems: lint, audit at the full matrix, read each rule's fix, fix, and justify what stays.",
            arguments: vec![PromptArg {
                name: "source",
                description: "The path of the `.qml` file or directory to review, or the QML itself.",
                required: true,
            }],
        },
        PromptDef {
            name: "localise",
            description: "Make every text the user reads translatable, and bring the catalogues up to date.",
            arguments: vec![paths(
                "The `.qml` files or directories to go through, for example `views`.",
            )],
        },
        PromptDef {
            name: "write-feature",
            description: "Describe a behaviour as a scenario the runner can run, before or beside the code that has it.",
            arguments: vec![PromptArg {
                name: "behaviour",
                description: "The behaviour in a sentence, for example `a box keeps its width when its parent grows`.",
                required: true,
            }],
        },
        PromptDef {
            name: "test-app",
            description: "Specify what existing QML does as @qt-parity scenarios, and run them on Ljós and on Qt.",
            arguments: vec![paths(
                "The `.qml` files to specify, for example `Main.qml`.",
            )],
        },
        PromptDef {
            name: "embed-in-rust",
            description: "Show QML from a Rust program: the App, native types, list models, threads that talk to the UI through UiHandle, and a headless test.",
            arguments: vec![PromptArg {
                name: "program",
                description: "What the program shows and does, in a sentence.",
                required: true,
            }],
        },
    ]
}

fn arg<'a>(args: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

fn required<'a>(
    args: &'a Map<String, Value>,
    prompt: &str,
    name: &str,
) -> Result<&'a str, SurfaceError> {
    arg(args, name).ok_or_else(|| {
        SurfaceError::invalid_argument(format!(
            "the prompt `{prompt}` needs the argument `{name}`, as a text that is not empty"
        ))
    })
}

pub(crate) fn get(name: &str, args: &Map<String, Value>) -> Result<PromptText, SurfaceError> {
    let (description, intro) = match name {
        "write-component" => {
            let component = required(args, name, "name")?;
            let purpose =
                arg(args, "purpose").map_or(String::new(), |p| format!(" It is for {p}."));
            (
                format!("Write the QML component {component}"),
                format!("Write the Ljós QML component {component}.{purpose}"),
            )
        }
        "port-from-qtquick" => {
            let source = required(args, name, "source")?;
            (
                "Port Qt Quick QML to Ljós".to_owned(),
                format!("Port this Qt Quick source to Ljós:\n\n{source}"),
            )
        }
        "add-accessibility" => {
            let paths = required(args, name, "paths")?;
            (
                format!("Make {paths} usable with a screen reader"),
                format!("Make the QML under {paths} usable by someone who cannot see it."),
            )
        }
        "design-review" => {
            let source = required(args, name, "source")?;
            (
                "Review QML for design problems".to_owned(),
                format!(
                    "Review this QML for UX, visual accessibility and motion problems, and fix \
                     them:\n\n{source}"
                ),
            )
        }
        "localise" => {
            let paths = required(args, name, "paths")?;
            (
                format!("Make the texts under {paths} translatable"),
                format!("Make every text a user reads under {paths} translatable."),
            )
        }
        "write-feature" => {
            let behaviour = required(args, name, "behaviour")?;
            (
                "Describe a behaviour as a scenario".to_owned(),
                format!("Describe this behaviour as a scenario the runner can run:\n\n{behaviour}"),
            )
        }
        "test-app" => {
            let paths = required(args, name, "paths")?;
            (
                format!("Specify what {paths} does, on Ljós and on Qt"),
                format!(
                    "Specify what {paths} does as scenarios that run on Ljós and on Qt, and run \
                     them."
                ),
            )
        }
        "embed-in-rust" => {
            let program = required(args, name, "program")?;
            (
                "Show QML from a Rust program".to_owned(),
                format!("Write the Rust program that shows QML with Ljós: {program}"),
            )
        }
        other => {
            let known: Vec<&str> = list().iter().map(|p| p.name).collect();
            return Err(SurfaceError::not_found(format!(
                "no prompt `{other}`; there are {}",
                known.join(", ")
            )));
        }
    };
    let body = body(name, Reader::Server).expect("every listed prompt has a body");
    Ok(PromptText {
        description,
        text: format!("{intro}\n\n{body}"),
    })
}

/// What a prompt says after it has said what the work is on, which is the
/// same whoever reads it but for where its documents are.
pub(crate) fn body(name: &str, reader: Reader) -> Option<String> {
    let brief = doc(reader, "docs/design/agent-brief.md");
    let guide = doc(reader, "docs/design/accessibility.md");
    let text = match name {
        "write-component" => format!(
            "1. `api_status` for every Qt Quick type and member you plan to use \
             (`QtQuick.Item.width`). A row that is `unsupported` or `dropped` says what to use \
             instead.\n\
             2. The behaviour before the code: `features_list` shows the scenarios nearest to \
             it; write a `.feature` that says what the component does. {SCENARIO_RULES}\n\
             3. `design_rules` lists the design rules; {brief} has them as one line each. Follow \
             every line.\n\
             4. Write the QML, then `qml_format`, `qml_check` and `qml_lint` on it.\n\
             5. `binding_tier_explain` on each file: take the rewrites it offers.\n\
             6. Run the scenarios with `features_run`, giving the path of the `.feature`, and \
             read `run_status`.\n\
             7. {VALIDATE}\n\n\
             {UNAVAILABLE}"
        ),
        "port-from-qtquick" => format!(
            "1. `qml_check` and `qml_lint` on the source first: each Error is a place to change.\n\
             2. For a type or member the source uses, `api_status` (`QtQuick.Item.width`) says \
             what Ljós has. Keep what is `supported`, stay inside the reason of a `partial` row, \
             and for `dropped` or `unsupported` use what the reason names. Where there is no row, \
             `type_lookup` says whether the type and member exist.\n\
             3. Change what a finding asks for and nothing else: what the files draw and do stays \
             as it was.\n\
             4. `qml_format` on the result, and `binding_tier_explain` on each file: take its \
             rewrites.\n\
             5. {VALIDATE}\n\n\
             Say at the end what could not be ported, and why. {UNAVAILABLE}"
        ),
        "add-accessibility" => format!(
            "1. `qml_lint` on the files. Findings whose code starts with `a11y-` are items a \
             screen reader cannot name; `design_rules` with the area `a11y` gives each rule's fix, \
             and {guide} its reasons.\n\
             2. Set `Accessible.name` as a person would say it, in `qsTr`, and `Accessible.role`; \
             `Accessible.ignored` for decoration. `type_lookup` on `QtQuick.Accessible` lists the \
             rest.\n\
             3. `qml_check` on the files you changed.\n\
             4. Hold it fixed: `features_list` with the tag expression `@a11y` shows how \
             accessibility is specified here; write a scenario that asks the accessibility tree \
             for each name, run it with `features_run` and read `run_status`.\n\
             5. {VALIDATE} A quiet lint is not proof: the audit sees what a screen reader is told \
             when the application runs.\n\n\
             {UNAVAILABLE}"
        ),
        "design-review" => {
            let rules = match reader {
                Reader::Server => format!("The brief, as {brief} has it:\n\n{AGENT_BRIEF}"),
                Reader::Skill => format!("The brief is {brief}."),
            };
            format!(
                "1. `design_rules` lists every rule with its severity, how it is checked and its \
                 fix. An Error is certainly wrong for some user, a Warning very likely, a Hint is \
                 advice. For colours `design_palette` and `design_contrast`, for motion \
                 `design_motion`.\n\
                 2. `qml_check` and `qml_lint` on the file or every file in the directory.\n\
                 3. Read the fix of each code you got before you change anything. Take Hints \
                 that cost nothing.\n\
                 4. `qml_format` on what you changed.\n\
                 5. {VALIDATE}\n\
                 6. A scenario holds a fix in place: `features_list` shows how the design audit is \
                 specified here, and `features_run` with `run_status` runs yours.\n\n\
                 {rules} Follow every line.\n\n\
                 {UNAVAILABLE}"
            )
        }
        "localise" => format!(
            "1. `qml_lint` on the files. `i18n-untranslated-string` is a text written out without \
             `qsTr`; `i18n-qstr-non-literal` is a `qsTr` whose argument no tool can extract.\n\
             2. Wrap each text in `qsTr`, with what varies put in by `.arg()`, not `+`.\n\
             3. `qml_format` and `qml_check` on the files you changed.\n\
             4. `ljos i18n extract` in a terminal brings the `.ts` catalogues up to date. If you \
             cannot run it, say that it is left to do.\n\
             5. {VALIDATE}\n\n\
             {UNAVAILABLE}"
        ),
        "write-feature" => format!(
            "1. `features_list` with a path or a name near the behaviour: it may be specified \
             already, and if not, it belongs in the file whose scenarios it lists.\n\
             2. `api_status` for the type or member the behaviour is about. Tag the scenario \
             `@api-Module.Type.member` to join the scenarios its row names; what Qt does too is \
             `@qt-parity`, what only Ljós does `@ljos-only`.\n\
             3. {SCENARIO_RULES}\n\
             4. One behaviour to a scenario; say what is seen, not how it is built.\n\
             5. Run it with `features_run`, giving the path of the file, and read `run_status`. A \
             scenario for behaviour not built yet fails: tag it `@backlog` and say so. Never \
             change what a scenario says to make it pass.\n\n\
             {UNAVAILABLE}"
        ),
        "test-app" => format!(
            "1. Read the files and list what a user sees and does: the texts, which controls are \
             enabled, what a press or a key changes. `features_list` shows scenarios the project \
             has already.\n\
             2. {SCENARIO_RULES}\n\
             3. Tag the feature `@qt-parity`, one behaviour to a scenario.\n\
             4. `features_run` with `paths` set to the file and `backend` set to `both`, then \
             `run_status`, waiting until the run has ended. A step that fails on Qt alone is \
             Ljós-only wording: find one that runs on both. A scenario that is wrong about the \
             file is fixed; one where Ljós and Qt differ is reported, never changed to pass.\n\n\
             The work is done when every scenario passes on both backends. {UNAVAILABLE}"
        ),
        "embed-in-rust" => {
            let native = doc(reader, "docs/site/rust/native-types.md");
            let models = doc(reader, "docs/site/rust/models.md");
            let threads = doc(reader, "docs/site/rust/threads-and-uihandle.md");
            let testing = doc(reader, "docs/site/rust/testing-rust.md");
            format!(
                "1. Depend on the `ljos` crate. While the program is written, its `dev-qml` \
                 feature and `App::qml_dir` compile the QML from disk at start-up, with no build \
                 script and no type dump. A program that ships compiles its QML in `build.rs` \
                 and embeds it with `ljos::include_qml!`; that needs `ljostypes.json`, which \
                 `LJOS_TYPES=write cargo test` writes, once and after every change to a native \
                 type. If you may not run that, ask the user to.\n\
                 2. A native type is a struct with `#[derive(QmlObject)]` and `#[qml_impl]`, \
                 registered with `App::register`: {native}.\n\
                 3. A list from Rust implements `QmlListModel` and is attached with \
                 `Engine::set_list_model`; the `ModelSignals` it returns tell the views what \
                 changed: {models}.\n\
                 4. Only the UI thread touches the `Engine`. Another thread takes the `UiHandle` \
                 of `setup.ui_handle()` in `App::run_with` and sends it messages or closures, \
                 which run on the UI thread: {threads}.\n\
                 5. Test headless with `App::build`, which builds the Engine and the root \
                 without a window: {testing}.\n\
                 6. `qml_format`, `qml_check` and `qml_lint` on the QML files, then `cargo build` \
                 and `cargo test`. Fix every Error and repeat; the work is done when all of them \
                 pass.\n\n\
                 {UNAVAILABLE}"
            )
        }
        _ => return None,
    };
    Some(text)
}

/// The tools a prompt's text names, in the order it first names them.
#[cfg(test)]
pub(crate) fn tools_named(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (index, word) in text.split('`').enumerate() {
        let tool = index % 2 == 1
            && word.contains('_')
            && word.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        if tool && !out.iter().any(|known| known == word) {
            out.push(word.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_without_what_it_needs_says_which_argument() {
        for prompt in list() {
            let Some(needed) = prompt.arguments.iter().find(|a| a.required) else {
                continue;
            };
            let error = get(prompt.name, &Map::new()).unwrap_err();
            assert!(error.message.contains(needed.name), "{}", error.message);
        }
    }

    /// A prompt that names a tool the server does not have sends an agent
    /// to call something that fails, so each name is held against the
    /// tools as they are registered, capability-gated ones included.
    #[test]
    fn every_tool_a_prompt_names_exists() {
        use ljos_mcp_core::McpSurface;

        let dir = tempfile::tempdir().unwrap();
        let server = crate::AuthorServer::new(
            crate::Project::open(dir.path()).unwrap(),
            std::sync::Arc::new(crate::StubFrontEnd),
        );
        let known: Vec<&str> = server.tools().all().map(|def| def.name).collect();
        for prompt in list() {
            let args: Map<String, Value> = prompt
                .arguments
                .iter()
                .map(|a| (a.name.to_owned(), Value::from("views")))
                .collect();
            let text = get(prompt.name, &args).unwrap().text;
            let named = tools_named(&text);
            assert!(named.len() >= 3, "{} names {named:?}", prompt.name);
            for tool in &named {
                assert!(
                    known.contains(&tool.as_str()),
                    "the prompt `{}` names `{tool}`, which is not a tool; there are {known:?}",
                    prompt.name
                );
            }
        }
    }

    /// What `tools_named` takes for a tool: a word in backticks with an
    /// underscore. A tool whose name had none would go unchecked above.
    #[test]
    fn every_tool_has_a_name_a_prompt_can_be_checked_for() {
        use ljos_mcp_core::McpSurface;

        let dir = tempfile::tempdir().unwrap();
        let server = crate::AuthorServer::new(
            crate::Project::open(dir.path()).unwrap(),
            std::sync::Arc::new(crate::StubFrontEnd),
        );
        for def in server.tools().all() {
            assert_eq!(tools_named(&format!("`{}`", def.name)), [def.name]);
        }
        assert!(
            tools_named("`width` and `Accessible.name` and `i18n-qstr-non-literal`").is_empty()
        );
    }

    #[test]
    fn descriptions_are_short() {
        for prompt in list() {
            assert!(prompt.description.len() <= 200, "{}", prompt.name);
        }
    }
}
