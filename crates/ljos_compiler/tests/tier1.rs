//! Tier-1 code generation (ARCHITECTURE.md §5.4): what is in the subset,
//! the shape of the programs, and the reasons the rest is tier 2. That a
//! program means what its JS function means is tested where both run, in
//! `ljos_engine`.

mod oracle;

use ljos_compiler::{
    Project, compile_file,
    tier1::{Coverage, Outcome, analyze},
};
use ljos_ir::{BindingKind, CompiledFile, Op, Tier1};
use oracle::Runtime;

fn compile(members: &str) -> CompiledFile {
    let source = format!(
        "import QtQuick\nItem {{\n    id: root\n    property real a: 1\n    property real b: 2\n    \
         property string s: \"x\"\n    function twice(v) {{ return v * 2 }}\n{members}\n}}\n"
    );
    let mut project = Project::new(Runtime::new());
    project.add("Main", &source);
    compile_file(&project, "Main").expect("compiles").file
}

/// The outcome of the `nth` expression of a kind, in source order.
fn outcome(file: &CompiledFile, kind: BindingKind, nth: usize) -> Outcome {
    let outcomes = analyze(file);
    file.exprs
        .iter()
        .zip(outcomes)
        .filter(|(binding, _)| binding.kind == kind)
        .nth(nth)
        .expect("the expression")
        .1
}

fn program(file: &CompiledFile, kind: BindingKind, nth: usize) -> Tier1 {
    match outcome(file, kind, nth) {
        Outcome::Tier1(program) => program,
        Outcome::Tier2(reason) => panic!("tier 2: {reason}"),
    }
}

fn reason(file: &CompiledFile, kind: BindingKind, nth: usize) -> String {
    match outcome(file, kind, nth) {
        Outcome::Tier1(program) => panic!("tier 1: {program:?}"),
        Outcome::Tier2(reason) => reason.to_string(),
    }
}

fn count(program: &Tier1, of: impl Fn(&Op) -> bool) -> usize {
    program.code.iter().filter(|op| of(op)).count()
}

#[test]
fn the_program_is_kept_in_the_binding_beside_its_function() {
    let file = compile("property var o: a + b\nproperty var t: console.log(a)");
    let bindings: Vec<_> = file
        .exprs
        .iter()
        .filter(|binding| binding.kind == BindingKind::Property)
        .collect();
    // Both have their JS function: tier 2 can run either.
    assert_ne!(bindings[0].js_fn, bindings[1].js_fn);
    let program = bindings[0].tier1.as_ref().expect("a + b is in the subset");
    assert_eq!(*program, self::program(&file, BindingKind::Property, 0));
    assert!(matches!(program.code.last(), Some(Op::Return { .. })));
    // No part of an expression outside the subset is compiled.
    assert_eq!(bindings[1].tier1, None);
}

#[test]
fn a_chain_of_operators_needs_no_register_for_each() {
    let short = compile("property var o: a + b");
    let long = compile("property var o: a + b * 2 - a / b + a * a - b + a + b + a + b + a + b");
    let short = program(&short, BindingKind::Property, 0);
    let long = program(&long, BindingKind::Property, 0);
    assert_eq!(count(&long, |op| matches!(op, Op::Binary { .. })), 13);
    assert!(
        long.registers <= short.registers + 1,
        "{} registers against {}",
        long.registers,
        short.registers
    );
}

#[test]
fn a_branch_not_taken_is_jumped_over() {
    let file =
        compile("property var o: a > 0 ? a : b\nproperty var p: a && b\nproperty var q: s ?? a");
    let conditional = program(&file, BindingKind::Property, 0);
    assert_eq!(
        count(&conditional, |op| matches!(op, Op::JumpIfNot { .. })),
        1
    );
    assert_eq!(count(&conditional, |op| matches!(op, Op::Jump { .. })), 1);
    // Each property is read at one place only: the read of `b` is behind
    // the jump, so it is a dependency only when it is taken.
    assert_eq!(count(&conditional, |op| matches!(op, Op::Get { .. })), 3);
    let and = program(&file, BindingKind::Property, 1);
    assert_eq!(count(&and, |op| matches!(op, Op::JumpIfNot { .. })), 1);
    let nullish = program(&file, BindingKind::Property, 2);
    assert_eq!(
        count(&nullish, |op| matches!(
            op,
            Op::JumpIfNullish { .. } | Op::JumpIf { .. } | Op::JumpIfNot { .. }
        )),
        1
    );
}

#[test]
fn a_function_of_the_file_is_inlined_behind_a_guard() {
    let file = compile("property var o: twice(a + 1) + 1");
    let program = program(&file, BindingKind::Property, 0);
    assert!(program.has_inlined_calls());
    let guard = program
        .code
        .iter()
        .position(|op| matches!(op, Op::Guard { .. }))
        .expect("a guard");
    let first_read = program
        .code
        .iter()
        .position(|op| matches!(op, Op::Get { .. }))
        .expect("a read");
    // JavaScript looks the function up before it evaluates the arguments.
    assert!(guard < first_read);
    assert_eq!(program.names[0], "twice");
    // The argument and what the call gives back go through the engine in
    // tier 2: each is passed once.
    assert_eq!(count(&program, |op| matches!(op, Op::Pass { .. })), 2);
    // `v * 2` and the two additions: the body is in the program.
    assert_eq!(count(&program, |op| matches!(op, Op::Binary { .. })), 3);
    assert_eq!(count(&program, |op| matches!(op, Op::Call { .. })), 0);
}

#[test]
fn a_function_calling_one_that_is_tier_2_is_tier_2_whole() {
    let file = compile(
        "function loop() { for (let i = 0; i < 3; i++) {} }\n\
         function wraps() { return loop() }\n\
         property var o: a + wraps()",
    );
    // `twice` is the first function of the file.
    assert_eq!(reason(&file, BindingKind::Function, 1), "statement: for");
    assert_eq!(
        reason(&file, BindingKind::Function, 2),
        "call of a function that is tier 2: statement: for"
    );
    assert!(
        reason(&file, BindingKind::Property, 0).starts_with("call of a function that is tier 2"),
    );
    assert_eq!(file.exprs.iter().filter(|b| b.tier1.is_some()).count(), 1);
}

#[test]
fn a_handler_may_end_with_one_write() {
    let file = compile(
        "onAChanged: b = a + 1\n\
         onBChanged: { a = 1; b = 2 }\n\
         onSChanged: { if (a > 1) b = 0; else b = a }",
    );
    let one = program(&file, BindingKind::Handler, 0);
    assert!(one.writes());
    let set = one
        .code
        .iter()
        .position(|op| matches!(op, Op::Set { .. }))
        .expect("the write");
    // Nothing that can fail follows the write: there is no taking it back.
    assert!(
        one.code[set + 1..]
            .iter()
            .all(|op| matches!(op, Op::Return { .. } | Op::Jump { .. } | Op::Move { .. })),
        "{:?}",
        one.code
    );
    assert_eq!(
        reason(&file, BindingKind::Handler, 1),
        "more than one effect"
    );
    // One write on each path is one write.
    let either = program(&file, BindingKind::Handler, 2);
    assert_eq!(count(&either, |op| matches!(op, Op::Set { .. })), 2);
}

#[test]
fn what_is_outside_the_subset_says_why() {
    let file = compile(
        "property var o: console.log(a)\n\
         property var p: s.split(\"x\")\n\
         property var r: new Date()\n\
         property var t: (function () { return 1 })\n\
         property var u: a in root",
    );
    let reasons: Vec<String> = file
        .exprs
        .iter()
        .zip(analyze(&file))
        .filter(|(binding, _)| binding.kind == BindingKind::Property)
        .map(|(_, outcome)| match outcome {
            Outcome::Tier1(program) => panic!("tier 1: {program:?}"),
            Outcome::Tier2(reason) => reason.to_string(),
        })
        .collect();
    assert_eq!(
        reasons,
        [
            "global function call: console.log",
            "method call: .split()",
            "new: Date",
            "function value",
            "operator: in"
        ]
    );
}

#[test]
fn coverage_counts_by_kind_and_ranks_the_reasons() {
    let file = compile(
        "property var o: a + b\n\
         property var p: console.log(a)\n\
         property var q: console.log(b)\n\
         property var r: s.split(\"x\")\n\
         onAChanged: b = a + 1\n\
         onBChanged: console.log(b)",
    );
    let mut coverage = Coverage::default();
    coverage.add("Main", &file);
    coverage.add("Again", &file);
    let (name, counts) = &coverage.files[0];
    assert_eq!(name, "Main");
    assert_eq!((counts.bindings.tier1, counts.bindings.total), (1, 4));
    assert_eq!((counts.handlers.tier1, counts.handlers.total), (1, 2));
    assert_eq!((counts.functions.tier1, counts.functions.total), (1, 1));
    assert_eq!((counts.all().tier1, counts.all().total), (3, 7));
    assert_eq!(coverage.total.all().total, 14);
    assert!((coverage.total.all().percent() - 300.0 / 7.0).abs() < 1e-9);
    assert_eq!(
        coverage.reasons(),
        [("global function call", 6), ("method call", 2)]
    );
    assert_eq!(
        coverage.details(),
        [
            ("global function call: console.log".to_string(), 6),
            ("method call: .split()".to_string(), 2)
        ]
    );
}

#[test]
fn the_reasons_say_which_kind_of_expression_they_are_for() {
    let file = compile(
        "property var p: console.log(a)\n\
         onAChanged: console.log(a)\n\
         onBChanged: console.log(b)\n\
         function log() { console.log(a) }",
    );
    let mut coverage = Coverage::default();
    coverage.add("Main", &file);
    let by_kind = coverage.reasons_by_kind();
    assert_eq!(by_kind.len(), 1);
    let (kind, fell) = by_kind[0];
    assert_eq!(kind, "global function call");
    assert_eq!((fell.bindings, fell.handlers, fell.functions), (1, 2, 1));
    assert_eq!(fell.all(), 4);
    assert_eq!(coverage.details_by_kind()[0].1, fell);
}
