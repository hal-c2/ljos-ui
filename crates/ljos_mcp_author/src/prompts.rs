//! Ways of working an agent can ask for by name. Each says which tools to
//! call and in what order.
//!
//! A word in backticks with an underscore in it is the name of a tool, and
//! of nothing else: a test holds every such word against the tools the
//! server has, so a prompt cannot send an agent to a tool that is not there.

use ljos_mcp_core::{PromptArg, PromptDef, PromptText, SurfaceError};
use serde_json::{Map, Value};

/// The brief every design prompt carries, as the document has it, so that
/// what an agent is told and what `docs/design/` says cannot drift apart.
const AGENT_BRIEF: &str = include_str!("../../../docs/design/agent-brief.md");

/// What ends every prompt that writes QML: the checks that read the design
/// rules. The audit tools are the inspector's and the command line's, not
/// this server's, so their names are written without backticks: a word in
/// backticks must be a tool here.
const DESIGN_CHECKS: &str = "Last, hold it to the design rules in docs/design/agent-brief.md: `design_rules` \
    lists them. Run `qml_lint` again on every file you touched, then audit the running scene at \
    the full matrix (light and dark, high contrast, 200 % text, the small window): `ljos audit \
    <file.qml>` in a terminal, or the inspector's tool app_audit with `matrix` set to `full` on a \
    running application. Fix every Error. Fix every Warning, or keep it with \
    `// ljos-allow(<code>): <reason>`. Do not call the work done while either run reports an \
    Error.";

/// Said at the end of every prompt: an answer an agent must not read past.
const UNAVAILABLE: &str = "A tool error with the code `unavailable`, or one that says a capability \
    is not granted, means that check did not happen: say so in your answer, do not call the work \
    checked.";

pub(crate) fn list() -> Vec<PromptDef> {
    let paths = |what: &'static str| PromptArg {
        name: "paths",
        description: what,
        required: true,
    };
    vec![
        PromptDef {
            name: "write-component",
            description: "Write a new QML component: the ledger, then the scenario, then the QML, then format, check, lint, tiers and a run.",
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
            description: "Port Qt Quick QML to Ljós: what the ledger says of every type and member it uses, the replacements, then format, check, lint and tiers.",
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
            description: "Review QML for UX, visual accessibility and motion problems: lint, audit at the full matrix, read each rule's guide, fix, and justify what stays.",
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
    match name {
        "write-component" => {
            let component = required(args, name, "name")?;
            let purpose = arg(args, "purpose").unwrap_or("(not stated; ask if it matters)");
            Ok(PromptText {
                description: format!("Write the QML component {component}"),
                text: format!(
                    "Write the Ljós QML component `{component}`. Purpose: {purpose}\n\n\
                     Work in this order. Each step names the tool that does it or checks it.\n\n\
                     1. The ledger first. Call `api_status` for every Qt Quick type and member you plan to \
                     use (`QtQuick.Item.width`). Do not build on a row that is `unsupported` or `dropped`: \
                     its reason says what to use instead.\n\
                     2. How it is done here. `docs_search` finds the section of the project's documents \
                     about what you are going to write (\"binding tiers\", \"accessibility\"); read the \
                     section it links before you decide.\n\
                     3. The behaviour before the code. `features_list` shows the scenarios nearest to yours. \
                     Read `ljos://features/steps` for the steps a scenario may use, and write a `.feature` \
                     that says what `{component}` does before any QML exists.\n\
                     4. The QML. Before you write it, `design_rules` lists the design rules (colour, text, \
                     layout, focus, motion, content, accessibility): docs/design/agent-brief.md has them as \
                     one line each, and each rule's guide says why. Give every item the user acts on an \
                     accessible name, and put every text the user reads in `qsTr`.\n\
                     5. `qml_format` on the new files, so that a later diff shows what changed and nothing \
                     else.\n\
                     6. `qml_check` on them, and fix every error it reports.\n\
                     7. `qml_lint` on them, and fix what it finds or say why it stays.\n\
                     8. `binding_tier_explain` on each file. A binding that runs as JavaScript where a \
                     simpler one would do comes with a rewrite: take it.\n\
                     9. Run your scenarios with `features_run`, giving the path of your `.feature`, and read \
                     how it went with `run_status`.\n\
                     10. {DESIGN_CHECKS}\n\n\
                     {UNAVAILABLE}"
                ),
            })
        }
        "port-from-qtquick" => {
            let source = required(args, name, "source")?;
            Ok(PromptText {
                description: "Port Qt Quick QML to Ljós".to_owned(),
                text: format!(
                    "Port this Qt Quick source to Ljós:\n\n{source}\n\n\
                     1. List every type, property, signal and method it uses.\n\
                     2. Call `api_status` for each as `Module.Type` or `Module.Type.member`. Keep what is \
                     `supported`. For `partial` read the reason and stay inside it. For `dropped` or \
                     `unsupported` use what the reason names instead, or report that nothing does.\n\
                     3. Where the ledger has no row, `type_lookup` says whether the type and the member \
                     exist at all, and what the member's type is.\n\
                     4. `docs_search` for how Ljós does what the source did another way (\"hot reload\", \
                     \"popups\", \"binding tiers\").\n\
                     5. Write the port, and lay it out with `qml_format`.\n\
                     6. `qml_check` on the result, and fix every error.\n\
                     7. `qml_lint` on it: code that came from elsewhere often has items nobody named and \
                     texts nobody can translate.\n\
                     8. `binding_tier_explain` on each file, and take the rewrites it offers.\n\n\
                     Say at the end what could not be ported, and why. {UNAVAILABLE}"
                ),
            })
        }
        "add-accessibility" => {
            let paths = required(args, name, "paths")?;
            Ok(PromptText {
                description: format!("Make {paths} usable with a screen reader"),
                text: format!(
                    "Make the QML under {paths} usable by someone who cannot see it.\n\n\
                     1. `qml_lint` on {paths}. The findings whose code starts with `a11y-` are items a \
                     screen reader cannot name; `ljos://lints` says what each rule wants. `design_rules` \
                     with the area `a11y` gives each rule's fix and its guide in docs/design/accessibility.md.\n\
                     2. For each finding decide what the item is for. `type_lookup` on `QtQuick.Accessible` \
                     lists what can be set; the ones that matter are `Accessible.name`, \
                     `Accessible.role`, `Accessible.description` and, for an item that is only decoration, \
                     `Accessible.ignored`.\n\
                     3. Write the names as a person would say them (\"Delete note\", not \"icon button\"), \
                     in `qsTr` so they are translated with the rest.\n\
                     4. `qml_check` on the files you changed.\n\
                     5. Hold it fixed. `features_list` with the tag expression `@a11y` shows how \
                     accessibility is specified here; write a scenario that asks the accessibility tree \
                     for each thing you named.\n\
                     6. Run it with `features_run` and read the result with `run_status`.\n\
                     7. {DESIGN_CHECKS}\n\n\
                     A lint that is quiet is not proof: it sees what is written in the file, not what \
                     a screen reader is told when the application runs. {UNAVAILABLE}"
                ),
            })
        }
        "design-review" => {
            let source = required(args, name, "source")?;
            Ok(PromptText {
                description: "Review QML for design problems".to_owned(),
                text: format!(
                    "Review this QML for UX, visual accessibility and motion problems, and fix them:\n\n\
                     {source}\n\n\
                     Work in this order.\n\n\
                     1. `design_rules` lists every rule with its severity, how it is checked and its fix. \
                     Severity is confidence: an Error is certainly wrong for some user and fails the \
                     checks, a Warning is very likely wrong, a Hint is advice. For colours \
                     `design_palette` and `design_contrast`, for motion `design_motion`.\n\
                     2. `qml_check`, then `qml_lint` on the file or on every file in the directory. Each \
                     finding has a code.\n\
                     3. Audit the scene as it is drawn, at the full matrix (light and dark, normal and \
                     high contrast, 200 % text, the smallest window): `ljos audit <file.qml>` in a \
                     terminal (its `--matrix` defaults to `full`), or the inspector's tool app_audit with \
                     `matrix` set to `full` on a running application (`ljos run --mcp`). A finding \
                     names the item by role, name and locator, its `file:line`, what was measured and \
                     the point of the matrix where it shows. Findings only the audit can make \
                     (contrast over the composited background, text cut off, a control off the window or \
                     covered) are not found by `qml_lint`.\n\
                     4. For each code you got, read its guide, `docs/design/<page>#<code>` (`docs_search` \
                     finds the section; the page is the code's first word: `a11y-` is accessibility.md, \
                     `text-` typography.md, `focus-` interaction.md), before you change anything.\n\
                     5. Fix every Error. Fix every Warning too, or keep it with a reason written where \
                     the finding is, `// ljos-allow(<code>): <reason>`; an allow with no reason is an \
                     Error, and an allow that keeps nothing is a Hint to delete. Take Hints that cost \
                     nothing.\n\
                     6. `qml_format` on what you changed, then `qml_check`, `qml_lint` and the audit again. \
                     Repeat until the lint and the audit report no Error and every Warning is fixed or \
                     justified.\n\
                     7. A scenario holds a fix in place: `features_list` shows how the design audit is \
                     specified here, and `features_run` with `run_status` runs yours.\n\n\
                     The brief, as docs/design/agent-brief.md has it; follow every line:\n\n\
                     {AGENT_BRIEF}\n\
                     {UNAVAILABLE}"
                ),
            })
        }
        "localise" => {
            let paths = required(args, name, "paths")?;
            Ok(PromptText {
                description: format!("Make the texts under {paths} translatable"),
                text: format!(
                    "Make every text a user reads under {paths} translatable.\n\n\
                     1. `qml_lint` on {paths}. `i18n-untranslated-string` is a text written out without \
                     `qsTr`; `i18n-qstr-non-literal` is a `qsTr` whose argument no tool can extract.\n\
                     2. `docs_search` for \"translation\" to read how catalogues are found and loaded \
                     here, and what `qsTr`, `qsTrId` and plural forms do.\n\
                     3. Wrap each text: `qsTr(\"Delete\")`. Put what varies in with `.arg()` and not \
                     with `+`, so a translator can move it. Leave out what is not language: ids, file \
                     names, the names of colours and fonts.\n\
                     4. `qml_format` on the files you changed.\n\
                     5. `qml_check` on them.\n\
                     6. In a terminal, `ljos i18n extract` brings the `.ts` catalogues up to date with \
                     the sources. It is a command, not a tool of this server: if you cannot run it, say \
                     that it is left to do.\n\n\
                     {UNAVAILABLE}"
                ),
            })
        }
        "write-feature" => {
            let behaviour = required(args, name, "behaviour")?;
            Ok(PromptText {
                description: "Describe a behaviour as a scenario".to_owned(),
                text: format!(
                    "Describe this behaviour as a scenario the runner can run:\n\n{behaviour}\n\n\
                     1. `features_list` with a path or a name near the behaviour: it may be specified \
                     already, and if not, the file it belongs in is the one whose scenarios it lists.\n\
                     2. Read `ljos://features/README.md` for the rules (the `# Sources:` block, the tags, \
                     the words a scenario does not use), and `docs_search` if a rule needs explaining.\n\
                     3. `api_status` for the type or member the behaviour is about. Its row names the \
                     scenarios that prove it: tag yours `@api-Module.Type.member` to join them. What Qt \
                     does too is tagged `@qt-parity`; what only Ljós does, `@ljos-only`.\n\
                     4. Write the scenario with the steps of `ljos://features/steps` and no others. One \
                     behaviour to a scenario; say what is seen, not how it is built.\n\
                     5. Run it with `features_run`, giving the path of the file, and read the result with \
                     `run_status`. A scenario for behaviour that is not built yet fails: tag it \
                     `@backlog` and say so. Never change what a scenario says to make it pass.\n\n\
                     {UNAVAILABLE}"
                ),
            })
        }
        other => {
            let known: Vec<&str> = list().iter().map(|p| p.name).collect();
            Err(SurfaceError::not_found(format!(
                "no prompt `{other}`; there are {}",
                known.join(", ")
            )))
        }
    }
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
