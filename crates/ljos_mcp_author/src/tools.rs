use std::sync::Arc;

use ljos_mcp_core::{CallCtx, NoArgs, SurfaceError, ToolDef, ToolRegistry, paginate, typed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Diagnostic, FrontEnd, Ledger, LedgerRow, Project, TypeInfo};

const PAGE: usize = 100;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FilesArgs {
    /// `.qml` files or directories, relative to the project root.
    paths: Vec<String>,
    /// Resume token from a previous truncated result.
    cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct DiagnosticsOut {
    diagnostics: Vec<Diagnostic>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TypeArgs {
    /// QML module, for example `QtQuick`.
    module: String,
    /// Type name, for example `Rectangle`.
    #[serde(rename = "type")]
    type_name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApiArgs {
    /// `Module.Type` or `Module.Type.member`, for example `QtQuick.Item.width`.
    query: String,
    cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct ApiOut {
    query: String,
    rows: Vec<LedgerRow>,
    next_cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct LedgerSummary {
    present: bool,
    rows: usize,
    modules: Vec<String>,
    /// Row count per status.
    by_status: std::collections::BTreeMap<String, usize>,
    /// Why the ledger could not be read, if it could not.
    error: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct ProjectInfo {
    ljos_version: &'static str,
    project_root: String,
    ledger: LedgerSummary,
    /// Whether the QML front end is linked in; when false `qml_check`,
    /// `qml_lint` and `type_lookup` return `unavailable`.
    front_end_wired: bool,
    capabilities: Vec<String>,
}

fn load_ledger(project: &Project) -> Result<Ledger, SurfaceError> {
    Ledger::load(&project.ledger_path()).map_err(|e| match e {
        crate::LedgerError::Read { .. } => SurfaceError::not_found(e.to_string()),
        other => SurfaceError::new(ljos_mcp_core::ErrorCode::Internal, other.to_string()),
    })
}

/// Expands directories to the `.qml` files below them, sorted so diagnostics
/// are reproducible.
fn expand_qml(
    project: &Project,
    paths: &[String],
) -> Result<Vec<std::path::PathBuf>, SurfaceError> {
    let mut out = Vec::new();
    for p in paths {
        let path = project.confine(p)?;
        if path.is_dir() {
            collect_qml(&path, &mut out);
        } else {
            out.push(path);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn collect_qml(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        // `file_type` does not follow symlinks, so a link cannot lead the walk out of the project.
        match entry.file_type() {
            Ok(t) if t.is_dir() => collect_qml(&path, out),
            Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "qml") => out.push(path),
            _ => {}
        }
    }
}

type Check = fn(&dyn FrontEnd, &[std::path::PathBuf]) -> Result<Vec<Diagnostic>, SurfaceError>;

fn diagnostics_tool(
    project: &Arc<Project>,
    front: &Arc<dyn FrontEnd>,
    run: Check,
) -> ljos_mcp_core::ToolHandler {
    let (project, front) = (project.clone(), front.clone());
    typed(move |_ctx: &CallCtx, args: FilesArgs| {
        let files = expand_qml(&project, &args.paths)?;
        let mut diagnostics = run(front.as_ref(), &files)?;
        for d in &mut diagnostics {
            // Front ends report canonical paths; agents think project-relative.
            d.file = project.display(std::path::Path::new(&d.file));
        }
        let (diagnostics, next_cursor) = paginate(diagnostics, args.cursor.as_deref(), PAGE)?;
        Ok(DiagnosticsOut {
            diagnostics,
            next_cursor,
        })
    })
}

pub(crate) fn registry(project: Arc<Project>, front: Arc<dyn FrontEnd>) -> ToolRegistry {
    // Fixed order: prompt caches key on the tool list (§19.6).
    ToolRegistry::new()
        .with(
            ToolDef::new::<FilesArgs>(
                "qml_check",
                "Parse and resolve QML files or directories; returns diagnostics (file, line, column, severity, code, message), paginated.",
            )
            .output::<DiagnosticsOut>(),
            diagnostics_tool(&project, &front, |f, files| f.check(files)),
        )
        .with(
            ToolDef::new::<FilesArgs>(
                "qml_lint",
                "Lint QML files or directories, including the accessibility audit; returns diagnostics, paginated.",
            )
            .output::<DiagnosticsOut>(),
            diagnostics_tool(&project, &front, |f, files| f.lint(files)),
        )
        .with(
            ToolDef::new::<TypeArgs>(
                "type_lookup",
                "Describe a QML type: base chain, default property, own properties, signals, methods, enums and attached type. Inherited members are in the types of base_chain.",
            )
            .output::<TypeInfo>(),
            {
                let front = front.clone();
                typed(move |_: &CallCtx, a: TypeArgs| front.type_lookup(&a.module, &a.type_name))
            },
        )
        .with(
            ToolDef::new::<ApiArgs>(
                "api_status",
                "Ledger status of Module.Type[.member]: supported, partial, dropped or unsupported, with reason and proving @api- scenarios.",
            )
            .output::<ApiOut>(),
            {
                let project = project.clone();
                typed(move |_: &CallCtx, a: ApiArgs| {
                    let ledger = load_ledger(&project)?;
                    let rows: Vec<LedgerRow> =
                        ledger.lookup(&a.query).into_iter().cloned().collect();
                    if rows.is_empty() {
                        return Err(SurfaceError::not_found(format!(
                            "`{}` is not in the ledger; use Module.Type or Module.Type.member",
                            a.query
                        ))
                        .with_details(serde_json::json!({ "modules": ledger.modules() })));
                    }
                    let (rows, next_cursor) = paginate(rows, a.cursor.as_deref(), PAGE)?;
                    Ok(ApiOut { query: a.query, rows, next_cursor })
                })
            },
        )
        .with(
            ToolDef::new::<NoArgs>(
                "project_info",
                "Ljós version, project root, ledger summary and the capabilities granted to this session.",
            )
            .output::<ProjectInfo>(),
            {
                let wired = front.wired();
                typed(move |ctx: &CallCtx, _: NoArgs| {
                    let ledger = match Ledger::load(&project.ledger_path()) {
                        Ok(l) => LedgerSummary {
                            present: true,
                            rows: l.rows().len(),
                            modules: l.modules().into_iter().map(str::to_owned).collect(),
                            by_status: l.counts().into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
                            error: None,
                        },
                        Err(e) => LedgerSummary {
                            present: false,
                            rows: 0,
                            modules: Vec::new(),
                            by_status: Default::default(),
                            error: Some(e.to_string()),
                        },
                    };
                    Ok(ProjectInfo {
                        ljos_version: env!("CARGO_PKG_VERSION"),
                        project_root: project.root().display().to_string(),
                        ledger,
                        front_end_wired: wired,
                        capabilities: ctx.caps.iter().map(|c| c.to_string()).collect(),
                    })
                })
            },
        )
}
