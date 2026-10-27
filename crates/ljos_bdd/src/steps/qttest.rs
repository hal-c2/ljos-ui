//! QML tests (QtTest) run by each backend's test runner, and their reports.

use cucumber::{then, when};
use gherkin::Step;

use crate::backend::testrun::{TestRun, expected_lines};
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

// `ljos check --stats`: the coverage tables after what it finds.
#[when("the files are checked with statistics")]
fn files_checked_stats(w: &mut LjosWorld) -> StepResult {
    w.run_tests(&TestRun {
        check: true,
        stats: true,
        ..TestRun::default()
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
