use ljos_mcp_core::{PromptArg, PromptDef, PromptText, SurfaceError};
use serde_json::{Map, Value};

pub(crate) fn list() -> Vec<PromptDef> {
    vec![
        PromptDef {
            name: "write-component",
            description: "Write a new QML component: ledger first, then the feature, then QML, then qml_check.",
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
            description: "Port a Qt Quick snippet: api_status for every type used, with replacements.",
            arguments: vec![PromptArg {
                name: "source",
                description: "The QML to port, or a path inside the project.",
                required: true,
            }],
        },
    ]
}

fn arg<'a>(args: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

pub(crate) fn get(name: &str, args: &Map<String, Value>) -> Result<PromptText, SurfaceError> {
    match name {
        "write-component" => {
            let component = arg(args, "name")
                .ok_or_else(|| SurfaceError::invalid_argument("`name` is required"))?;
            let purpose = arg(args, "purpose").unwrap_or("(not stated; ask if it matters)");
            Ok(PromptText {
                description: format!("Write the QML component {component}"),
                text: format!(
                    "Write the Ljós QML component `{component}`. Purpose: {purpose}\n\n\
                     Work in this order:\n\
                     1. Ledger first. Call `api_status` for every Qt Quick type and member you plan to use. \
                     Do not use `unsupported` or `dropped` rows; the reason says what to use instead.\n\
                     2. Feature second. Read `ljos://features/steps` and write a `.feature` scenario that describes \
                     the behaviour before any QML exists.\n\
                     3. QML third. Give every actionable item an accessible name.\n\
                     4. Finish with `qml_check` and `qml_lint` on the new files and fix every error. \
                     If either reports `unavailable`, say so instead of claiming the file is clean."
                ),
            })
        }
        "port-from-qtquick" => {
            let source = arg(args, "source")
                .ok_or_else(|| SurfaceError::invalid_argument("`source` is required"))?;
            Ok(PromptText {
                description: "Port a Qt Quick snippet to Ljós".to_owned(),
                text: format!(
                    "Port this Qt Quick source to Ljós:\n\n{source}\n\n\
                     1. List every type, property, signal and method it uses.\n\
                     2. Call `api_status` for each as `Module.Type[.member]`.\n\
                     3. For `supported` rows keep the code. For `partial` read the reason and stay inside it. \
                     For `dropped` or `unsupported` rows use the replacement named in the reason, or report that none exists.\n\
                     4. Run `qml_check` on the result."
                ),
            })
        }
        other => Err(SurfaceError::not_found(format!("no prompt `{other}`"))),
    }
}
