use std::sync::Arc;

use ljos_mcp_core::{
    CallCtx, Cap, NoArgs, SurfaceError, ToolAnnotations, ToolDef, ToolRegistry, typed,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    Diagnostic, FrontEnd, Ledger, LedgerRow, Project, TypeInfo, api, docs, features, format, runs,
    tiers,
};

const PAGE: usize = 100;
/// What the items of one page may add up to as JSON: under half the
/// default response cap, which is what one copy of an answer may be.
pub(crate) const BUDGET: usize = 16 * 1024;

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
    Ledger::load(&project.ledger_file()?).map_err(|e| match e {
        crate::LedgerError::Read { .. } => SurfaceError::not_found(e.to_string()),
        other => SurfaceError::new(ljos_mcp_core::ErrorCode::Internal, other.to_string()),
    })
}

/// Directories a walk does not go into: build output and what tools keep
/// for themselves. A path that is named is taken whatever it is called.
pub(crate) fn skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "target" | "node_modules" | "build")
        || name.starts_with("build-")
        || name.starts_with("cmake-build")
}

/// Expands directories to the `.qml` files below them, sorted so answers
/// are reproducible and a cursor means the same file on the next call.
pub(crate) fn expand_qml(
    project: &Project,
    paths: &[String],
) -> Result<Vec<std::path::PathBuf>, SurfaceError> {
    let mut out = Vec::new();
    for p in paths {
        let path = project.confine(p)?;
        if path.is_dir() {
            collect(&path, "qml", &mut out);
        } else if path.extension().is_some_and(|e| e == "qml") {
            out.push(path);
        } else {
            return Err(SurfaceError::invalid_argument(format!(
                "`{p}` is not a `.qml` file or a directory"
            )));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// The files with `extension` below `dir`.
pub(crate) fn collect(dir: &std::path::Path, extension: &str, out: &mut Vec<std::path::PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        // `file_type` does not follow symlinks, so a link cannot lead the walk out of the project.
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                if !entry.file_name().to_str().is_some_and(skipped_dir) {
                    collect(&path, extension, out);
                }
            }
            Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == extension) => {
                out.push(path);
            }
            _ => {}
        }
    }
}

/// Where a `cursor` says to go on from.
pub(crate) fn cursor(cursor: Option<&str>) -> Result<usize, SurfaceError> {
    match cursor {
        None => Ok(0),
        Some(c) => c.parse().map_err(|_| {
            SurfaceError::invalid_argument(format!(
                "bad cursor `{c}`: pass the `next_cursor` of the previous answer unchanged"
            ))
        }),
    }
}

/// The items from `cursor` on that fit in `budget` bytes of JSON, at most
/// `limit` of them and at least one, and the cursor of the rest.
///
/// The response cap of the core (§19.6) halves the longest list of an
/// answer that is too large, and cannot move a tool's own cursor back to
/// match: what it cut would be skipped by the next call. So a tool that
/// pages keeps its pages under the cap itself.
pub(crate) fn page<T: Serialize>(
    items: Vec<T>,
    cursor_text: Option<&str>,
    limit: usize,
    budget: usize,
) -> Result<(Vec<T>, Option<String>), SurfaceError> {
    let start = cursor(cursor_text)?;
    let total = items.len();
    let mut out = Vec::new();
    let mut spent = 0;
    for item in items.into_iter().skip(start) {
        let size = serde_json::to_vec(&item).map_or(0, |bytes| bytes.len() + 1);
        if !out.is_empty() && (out.len() == limit.max(1) || spent + size > budget) {
            break;
        }
        spent += size;
        out.push(item);
    }
    let next = start.min(total) + out.len();
    Ok((out, (next < total).then(|| next.to_string())))
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
        let (diagnostics, next_cursor) = page(diagnostics, args.cursor.as_deref(), PAGE, BUDGET)?;
        Ok(DiagnosticsOut {
            diagnostics,
            next_cursor,
        })
    })
}

pub(crate) fn registry(
    project: Arc<Project>,
    front: Arc<dyn FrontEnd>,
    runs: &Arc<runs::Runs>,
) -> ToolRegistry {
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
                "Lint QML files or directories: what `qml_check` reports and the accessibility and translation lints of `ljos check`, under the same codes (`ljos://lints` says what each wants); returns diagnostics, paginated.",
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
                    // A ledger that is not there is an empty one: the
                    // tables of types answer all the same.
                    let ledger = match load_ledger(&project) {
                        Ok(ledger) => ledger,
                        Err(e) if e.code == ljos_mcp_core::ErrorCode::NotFound => Ledger::default(),
                        Err(e) => return Err(e),
                    };
                    let mut rows: Vec<LedgerRow> =
                        ledger.lookup(&a.query).into_iter().cloned().collect();
                    if rows.is_empty()
                        && let Some((module, ty, member)) = api::split_query(&a.query)
                    {
                        rows.extend(api::status(Some(&ledger), &module, &ty, member.as_deref()));
                    }
                    if rows.is_empty() {
                        let mut message = format!(
                            "`{}` is not in the ledger or in Ljós's type table; use Module.Type or Module.Type.member",
                            a.query
                        );
                        if !qt_types::has_qt_table() {
                            message.push_str(&format!(" ({})", qt_types::WITHOUT_QT_TABLE));
                        }
                        return Err(SurfaceError::not_found(message)
                            .with_details(serde_json::json!({ "modules": api::modules() })));
                    }
                    let (rows, next_cursor) = page(rows, a.cursor.as_deref(), PAGE, BUDGET)?;
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
                let project = project.clone();
                typed(move |ctx: &CallCtx, _: NoArgs| {
                    let loaded = project
                        .ledger_file()
                        .map_err(|e| e.message)
                        .and_then(|path| Ledger::load(&path).map_err(|e| e.to_string()));
                    let ledger = match loaded {
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
                            error: Some(e),
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
        .with(
            ToolDef::new::<format::FormatArgs>("qml_format", format::DESCRIPTION)
                .output::<format::FormatOut>()
                // Writes only when asked to and allowed to; the annotation
                // says what it can do, not what it does by default.
                .annotated(ToolAnnotations::WRITE),
            format::handler(&project),
        )
        .with(
            ToolDef::new::<tiers::TierArgs>("binding_tier_explain", tiers::DESCRIPTION)
                .output::<tiers::TierOut>(),
            tiers::handler(&project),
        )
        .with(
            ToolDef::new::<docs::SearchArgs>("docs_search", docs::DESCRIPTION)
                .output::<docs::SearchOut>(),
            docs::handler(&project),
        )
        .with(
            ToolDef::new::<features::ListArgs>("features_list", features::LIST)
                .output::<features::ListOut>(),
            features::handler(&project),
        )
        .with(
            ToolDef::new::<runs::RunArgs>("features_run", runs::RUN)
                .output::<runs::Started>()
                .requires(Cap::Run)
                // Starts a process; asked twice, it starts two.
                .annotated(ToolAnnotations {
                    read_only: false,
                    destructive: false,
                    idempotent: false,
                    open_world: false,
                }),
            runs::handler(runs),
        )
        // The two below wait on a run, so the server answers them itself
        // (`AuthorServer::call_tool`); here they are listed and checked.
        .declare(
            ToolDef::new::<runs::StatusArgs>("run_status", runs::STATUS)
                .output::<runs::StatusOut>(),
        )
        .declare(
            ToolDef::new::<runs::CancelArgs>("run_cancel", runs::CANCEL)
                .output::<runs::StatusOut>()
                .annotated(ToolAnnotations::WRITE),
        )
}
