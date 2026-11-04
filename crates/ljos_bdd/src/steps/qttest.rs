//! QML tests (QtTest) run by each backend's test runner, and their reports.

use cucumber::{then, when};
use gherkin::Step;

use crate::backend::testrun::{TestRun, declarations, expected_lines};
use crate::value;
use crate::world::LjosWorld;

use super::{StepResult, docstring};

//
// These steps run the staged `tst_*.qml` files with a test runner and read
// what it printed. Nothing is loaded as an application for them.

#[when("the tests are run")]
fn tests_run(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun::default())
}

#[when(regex = r#"^the tests are run with the import path "([^"]+)"$"#)]
fn tests_run_with_import(w: &mut LjosWorld, dir: String) -> StepResult {
    w.run_tests(&TestRun {
        import_paths: vec![dir],
        ..TestRun::default()
    })
}

// The runner given a directory of the files, as `ljos test tests` is from
// a project whose tests import its other directories.
#[when(regex = r#"^the tests in "([^"]+)" are run$"#)]
fn tests_run_in(w: &mut LjosWorld, dir: String) -> StepResult {
    w.run_tests(&TestRun {
        dir: Some(dir),
        ..TestRun::default()
    })
}

#[when(regex = r#"^the tests in "([^"]+)" are run with the import path "([^"]+)"$"#)]
fn tests_run_in_with_import(w: &mut LjosWorld, dir: String, import: String) -> StepResult {
    w.run_tests(&TestRun {
        dir: Some(dir),
        import_paths: vec![import],
        ..TestRun::default()
    })
}

// `TZ`, as a program is started in a zone: an IANA name (`Europe/Berlin`)
// or a POSIX rule.
#[when(regex = r#"^the tests are run in the time zone "([^"]+)"$"#)]
fn tests_run_in_zone(w: &mut LjosWorld, zone: String) -> StepResult {
    w.run_tests(&TestRun {
        time_zone: Some(zone),
        ..TestRun::default()
    })
}

#[when(regex = r#"^the tests matching "([^"]+)" are run$"#)]
fn tests_run_matching(w: &mut LjosWorld, filter: String) -> StepResult {
    w.run_tests(&TestRun {
        filters: vec![filter],
        ..TestRun::default()
    })
}

// `ljos check` reads the staged files as one project; what it says is kept
// as a report, like a test runner's.
#[when("the files are checked")]
fn files_checked(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun {
        check: true,
        ..TestRun::default()
    })
}

// `ljos migrate --write`, then `ljos check`: what a migrated project
// compiles to, after what the migration said it did.
#[when("the files are migrated and checked")]
fn files_migrated_checked(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun {
        check: true,
        migrate: true,
        ..TestRun::default()
    })
}

// `ljos check --stats`: the coverage tables after what it finds.
#[when("the files are checked with statistics")]
fn files_checked_stats(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun {
        check: true,
        stats: true,
        ..TestRun::default()
    })
}

// `ljos check --tsc`: the checker's findings, then TypeScript's at the
// lines of the QML files they are in.
#[when("the files are checked with TypeScript")]
fn files_checked_tsc(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun {
        check: true,
        tsc: true,
        ..TestRun::default()
    })
}

/// What `ljos check --tsc` says when the machine has no TypeScript.
const NO_TSC: &str = "SKIPPED: no tsc";

/// The lines of a report that are TypeScript's: their code is `TS` and a
/// number.
fn typescript_lines(lines: &[String]) -> Vec<&String> {
    lines.iter().filter(|l| l.contains("[TS")).collect()
}

#[then("TypeScript reports the lines:")]
fn typescript_reports_lines(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.lines.iter().any(|l| l == NO_TSC) {
            return Ok(());
        }
        let said = typescript_lines(&report.lines);
        if said.len() >= want.len()
            && said
                .windows(want.len())
                .any(|w| w.iter().copied().eq(&want))
        {
            Ok(())
        } else {
            Err(format!(
                "these lines are not what TypeScript reports; the check says\n{}",
                report.text()
            ))
        }
    })
}

#[then("TypeScript reports nothing")]
fn typescript_reports_nothing(w: &mut LjosWorld) -> StepResult {
    w.each_test_report(|report| {
        // The count says the check ran: a refused command line has no
        // TypeScript in it either.
        let ran = report
            .lines
            .iter()
            .any(|l| l == NO_TSC || l.contains(" files, "));
        if ran && typescript_lines(&report.lines).is_empty() {
            Ok(())
        } else {
            Err(format!("TypeScript reports\n{}", report.text()))
        }
    })
}

// `ljos types --dts`: no files are needed, the API is the table's.
#[when("the declarations of Ljós's API are written")]
fn declarations_written(w: &mut LjosWorld) -> StepResult {
    w.ljos_report(declarations().map_err(|e| e.to_string())?);
    Ok(())
}

#[then(regex = r#"^the declarations have the line "(.+)"$"#)]
fn declarations_have_line(w: &mut LjosWorld, line: String) -> StepResult {
    w.each_test_report(|report| {
        if report.lines.iter().any(|l| l.trim() == line) {
            Ok(())
        } else {
            Err(format!("no line is {line:?}"))
        }
    })
}

/// The lines between `interface NAME ` and the brace that closes it.
fn declaration<'r>(lines: &'r [String], name: &str) -> Result<&'r [String], String> {
    let head = format!("interface {name} ");
    let start = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&head))
        .ok_or_else(|| format!("nothing declares the interface {name}"))?;
    let indent = lines[start].len() - lines[start].trim_start().len();
    let end = lines[start..]
        .iter()
        .position(|l| l.len() > indent && &l[indent..] == "}")
        .map_or(lines.len(), |i| start + i);
    Ok(&lines[start + 1..end])
}

#[then(regex = r#"^the declaration of "([^"]+)" has the member "(.+)"$"#)]
fn declaration_has_member(w: &mut LjosWorld, name: String, member: String) -> StepResult {
    w.each_test_report(|report| {
        let body = declaration(&report.lines, &name)?;
        if body.iter().any(|l| l.trim() == member) {
            Ok(())
        } else {
            Err(format!("{name} declares\n{}", body.join("\n")))
        }
    })
}

#[then(regex = r#"^the declaration of "([^"]+)" has no member "([^"]+)"$"#)]
fn declaration_lacks_member(w: &mut LjosWorld, name: String, member: String) -> StepResult {
    w.each_test_report(|report| {
        let body = declaration(&report.lines, &name)?;
        let named = |line: &String| {
            let line = line.trim().trim_start_matches("readonly ");
            line.strip_prefix(&member)
                .is_some_and(|rest| rest.starts_with([':', '(', '?']))
        };
        match body.iter().find(|l| named(l)) {
            None => Ok(()),
            Some(line) => Err(format!("{name} declares {}", line.trim())),
        }
    })
}

#[then("the check reports the lines:")]
fn check_reports_lines(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.has_lines(&want) {
            Ok(())
        } else {
            Err(format!(
                "these lines are not in what the check reports, which is\n{}",
                report.text()
            ))
        }
    })
}

#[then("the check reports:")]
fn check_reports(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.lines == want {
            Ok(())
        } else {
            Err(format!("the check reports\n{}", report.text()))
        }
    })
}

#[then("the test report is:")]
fn test_report_is(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.lines == want {
            Ok(())
        } else {
            Err(format!("the report is\n{}", report.text()))
        }
    })
}

#[then("the test report has the lines:")]
fn test_report_has(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = expected_lines(&docstring(step)?);
    w.each_test_report(|report| {
        if report.has_lines(&want) {
            Ok(())
        } else {
            Err(format!(
                "these lines are not in the report, which is\n{}",
                report.text()
            ))
        }
    })
}

#[then(regex = r"^the test run exits with status (\d+)$")]
fn test_run_status(w: &mut LjosWorld, want: i32) -> StepResult {
    w.each_test_report(|report| {
        if report.status == want {
            Ok(())
        } else {
            Err(format!(
                "the run exited with status {}; the report is\n{}",
                report.status,
                report.text()
            ))
        }
    })
}

// The machine-readable report, by JSON pointer (`/totals/failed`).
#[then(regex = r#"^the test results' "([^"]*)" is (.+)$"#)]
fn test_results_value(w: &mut LjosWorld, pointer: String, want: String) -> StepResult {
    let want = value::parse(&want)?;
    w.each_test_report(|report| {
        let json = report
            .json
            .as_ref()
            .ok_or("this runner writes no machine-readable report")?;
        match json.pointer(&pointer) {
            Some(actual) if value::same(&want, actual) => Ok(()),
            Some(actual) => Err(format!("{pointer} is {actual}, expected {want}")),
            None => Err(format!("the results have no {pointer}: {json}")),
        }
    })
}
