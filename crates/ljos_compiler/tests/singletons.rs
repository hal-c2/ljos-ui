//! Singletons of the project: how a file that reads one is compiled, and
//! which of the singleton's functions a tier-1 program may call in place.

mod oracle;

use ljos_compiler::{Project, compile_file, tier1::Outcome, tier1::analyze};
use ljos_ir::{Arg, BindingKind, CompiledFile, Op, Tier1};
use oracle::Runtime;

const THEME: &str = r#"pragma Singleton
import QtQuick
QtObject {
    property int base: 4
    function scaled(n) { return n * base }
    function grow() { base = base + 1 }
    function spelled(n) { return [n].join("") }
}
"#;

fn project(main: &str) -> Project<Runtime> {
    let mut project = Project::new(Runtime::new());
    project.add("Theme", THEME);
    project.add("Main", main);
    project
}

fn compile(main: &str) -> CompiledFile {
    compile_file(&project(main), "Main").expect("compiles").file
}

fn program(file: &CompiledFile, nth: usize) -> Option<Tier1> {
    let outcomes = analyze(file);
    let (_, outcome) = file
        .exprs
        .iter()
        .zip(outcomes)
        .filter(|(binding, _)| binding.kind == BindingKind::Property)
        .nth(nth)
        .expect("the binding");
    match outcome {
        Outcome::Tier1(program) => Some(program),
        Outcome::Tier2(_) => None,
    }
}

fn invokes(program: &Tier1) -> usize {
    program
        .code
        .iter()
        .filter(|op| matches!(op, Op::Invoke { .. }))
        .count()
}

#[test]
fn a_singleton_of_the_project_is_named_by_its_type() {
    let file = compile("import QtQuick\nItem { property int a: Theme.base }\n");
    let binding = file
        .exprs
        .iter()
        .find(|b| b.kind == BindingKind::Property)
        .expect("the binding");
    assert!(
        binding
            .params
            .iter()
            .any(|param| matches!(param.arg, Arg::Singleton(_))),
        "{:?}",
        binding.params
    );
    assert!(program(&file, 0).is_some(), "a member read is tier 1");
}

#[test]
fn a_function_that_writes_nothing_is_called_in_place() {
    let file = compile("import QtQuick\nItem { property int a: Theme.scaled(3) }\n");
    let program = program(&file, 0).expect("tier 1");
    assert_eq!(invokes(&program), 1);
    assert_eq!(file.invocable.len(), 1);
    assert_eq!(file.invocable[0].functions, ["scaled"]);
}

#[test]
fn a_function_that_writes_or_is_tier_2_is_not() {
    let writes = compile("import QtQuick\nItem { property var a: Theme.grow() }\n");
    assert!(program(&writes, 0).is_none(), "it writes a property");
    let tier2 = compile("import QtQuick\nItem { property var a: Theme.spelled(3) }\n");
    assert!(program(&tier2, 0).is_none(), "it has no program of its own");
}

#[test]
fn a_file_that_reads_no_singleton_takes_nothing_of_one() {
    let file = compile("import QtQuick\nItem { property int a: 1 + 2 }\n");
    assert!(file.invocable.is_empty(), "{:?}", file.invocable);
}
