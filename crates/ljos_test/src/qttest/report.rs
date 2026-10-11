//! What a test run reports: QtTest's plain text lines as they happen, and
//! the same results as data.
//!
//! The text has the shape Qt's runner prints (doc.qt.io/qt-6/qtest-overview.html,
//! "Logging Options", and what `qmltestrunner` 6.11.2 prints for the
//! scenarios in `features/tooling/qttest-*.feature`): one line per result,
//! `   Actual   ()`, `   Expected ()` and `   Loc:` lines under a failure,
//! and a `Totals:` line at the end. The identity of a function has no
//! program name in front: `Case::function(tag)`.

use std::time::Instant;

use serde_json::{Value as Json, json};

/// How one run of a test function (or of `initTestCase`, `cleanupTestCase`)
/// ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::Fail => "fail",
            Outcome::Skip => "skip",
        }
    }
}

/// What a line of the report says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    /// A check did not hold.
    Fail,
    /// A check failed as `expectFail` said it would.
    ExpectedFail,
    /// A check held although `expectFail` said it would not.
    UnexpectedPass,
    Skip,
    /// `TestCase.warn`.
    Warning,
    /// `console.log` and `console.debug`.
    Debug,
    /// `console.info`.
    Info,
    /// `console.warn` and the engine's own warnings.
    Warn,
    /// `console.error`.
    Critical,
}

impl MessageKind {
    /// The word the text report starts the line with, padded as Qt pads it.
    fn label(self) -> &'static str {
        match self {
            MessageKind::Fail => "FAIL!  ",
            MessageKind::ExpectedFail => "XFAIL  ",
            MessageKind::UnexpectedPass => "XPASS  ",
            MessageKind::Skip => "SKIP   ",
            MessageKind::Warning => "WARNING",
            MessageKind::Debug => "QDEBUG ",
            MessageKind::Info => "QINFO  ",
            MessageKind::Warn => "QWARN  ",
            MessageKind::Critical => "QCRITICAL",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Fail => "fail",
            MessageKind::ExpectedFail => "xfail",
            MessageKind::UnexpectedPass => "xpass",
            MessageKind::Skip => "skip",
            MessageKind::Warning => "warning",
            MessageKind::Debug => "debug",
            MessageKind::Info => "info",
            MessageKind::Warn => "warn",
            MessageKind::Critical => "critical",
        }
    }
}

/// One thing said about a test function while it ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: MessageKind,
    pub message: String,
    /// The two sides of a comparison, as they are printed.
    pub actual: Option<String>,
    pub expected: Option<String>,
    /// The file the check was written in, as a path.
    pub file: Option<String>,
    /// 1-based.
    pub line: Option<u32>,
}

impl Message {
    pub fn new(kind: MessageKind, message: impl Into<String>) -> Self {
        Message {
            kind,
            message: message.into(),
            actual: None,
            expected: None,
            file: None,
            line: None,
        }
    }

    pub fn sides(mut self, actual: impl Into<String>, expected: impl Into<String>) -> Self {
        self.actual = Some(actual.into());
        self.expected = Some(expected.into());
        self
    }

    pub fn at(mut self, place: Option<(String, u32)>) -> Self {
        if let Some((file, line)) = place {
            self.file = Some(file);
            self.line = Some(line);
        }
        self
    }
}

/// One run of a test function: one row of its data, or the function alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRecord {
    /// `TestCase.name`; empty for a case without one.
    pub case: String,
    pub function: String,
    /// The row's tag (`row 1` for a row without one).
    pub tag: Option<String>,
    pub outcome: Outcome,
    pub messages: Vec<Message>,
}

impl TestRecord {
    /// `Case::function(tag)`, as the text report names it.
    pub fn identity(&self) -> String {
        identity(&self.case, &self.function, self.tag.as_deref())
    }
}

/// `Case::function(tag)`. A case without a name has no `Case::` in front.
pub fn identity(case: &str, function: &str, tag: Option<&str>) -> String {
    let tag = tag.unwrap_or_default();
    if case.is_empty() {
        format!("{function}({tag})")
    } else {
        format!("{case}::{function}({tag})")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
}

/// The results of a run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub totals: Totals,
    pub tests: Vec<TestRecord>,
}

impl Report {
    /// Whether nothing failed.
    pub fn passed(&self) -> bool {
        self.totals.failed == 0
    }

    /// What a process that ran the tests exits with: how many failed, as
    /// Qt's runner does, kept below the values a shell gives a meaning.
    pub fn exit_code(&self) -> u8 {
        self.totals.failed.min(100) as u8
    }

    /// The results as data:
    ///
    /// ```json
    /// { "totals": { "passed": 2, "failed": 1, "skipped": 0 },
    ///   "tests": [ { "case": "C", "function": "test_x", "tag": "row 0",
    ///                "outcome": "fail",
    ///                "messages": [ { "kind": "fail", "message": "…",
    ///                                "actual": "1", "expected": "2",
    ///                                "file": "/dir/tst_c.qml", "line": 7 } ] } ] }
    /// ```
    ///
    /// `tag`, `actual`, `expected`, `file` and `line` are left out when
    /// there is none.
    pub fn to_json(&self) -> Json {
        let tests: Vec<Json> = self
            .tests
            .iter()
            .map(|t| {
                let messages: Vec<Json> = t
                    .messages
                    .iter()
                    .map(|m| {
                        let mut o = json!({ "kind": m.kind.as_str(), "message": m.message });
                        let fields = [
                            ("actual", m.actual.clone().map(Json::from)),
                            ("expected", m.expected.clone().map(Json::from)),
                            ("file", m.file.clone().map(Json::from)),
                            ("line", m.line.map(Json::from)),
                        ];
                        for (key, value) in fields {
                            if let Some(value) = value {
                                o[key] = value;
                            }
                        }
                        o
                    })
                    .collect();
                let mut o = json!({
                    "case": t.case,
                    "function": t.function,
                    "outcome": t.outcome.as_str(),
                    "messages": messages,
                });
                if let Some(tag) = &t.tag {
                    o["tag"] = Json::from(tag.clone());
                }
                o
            })
            .collect();
        json!({
            "totals": {
                "passed": self.totals.passed,
                "failed": self.totals.failed,
                "skipped": self.totals.skipped,
            },
            "tests": tests,
        })
    }
}

/// Where the lines of the text report go.
pub type LineSink = Box<dyn FnMut(&str)>;

/// Writes the text report line by line as results come in, and keeps the
/// results.
pub(crate) struct Reporter {
    sink: LineSink,
    program: String,
    started: Option<Instant>,
    report: Report,
    /// The function that is running, when one is.
    open: Option<TestRecord>,
}

impl Reporter {
    pub(crate) fn new(program: impl Into<String>, sink: LineSink) -> Self {
        Reporter {
            sink,
            program: program.into(),
            started: None,
            report: Report::default(),
            open: None,
        }
    }

    /// One line of text. The first one is preceded by the banner, so a
    /// run that reports nothing prints nothing, as Qt's does.
    fn line(&mut self, text: &str) {
        if self.started.is_none() {
            self.started = Some(Instant::now());
            let banner = format!("********* Start testing of {} *********", self.program);
            (self.sink)(&banner);
            let config = format!(
                "Config: Using ljos_test {}, virtual time",
                env!("CARGO_PKG_VERSION")
            );
            (self.sink)(&config);
        }
        (self.sink)(text);
    }

    /// A function starts: what is said from now on is said about it.
    pub(crate) fn begin(&mut self, case: &str, function: &str, tag: Option<&str>) {
        self.open = Some(TestRecord {
            case: case.to_owned(),
            function: function.to_owned(),
            tag: tag.map(str::to_owned),
            outcome: Outcome::Pass,
            messages: Vec::new(),
        });
    }

    /// Whether the running function has failed so far.
    pub(crate) fn has_failed(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|t| t.outcome == Outcome::Fail)
    }

    /// Says `message` about the running function, or about no function
    /// (`UnknownTestFunc()`, Qt's name for it) when none is running.
    pub(crate) fn say(&mut self, message: Message) {
        let id = match &self.open {
            Some(t) => t.identity(),
            None => "UnknownTestFunc()".to_owned(),
        };
        let label = message.kind.label();
        self.line(&format!("{label}: {id} {}", message.message));
        if let (Some(actual), Some(expected)) = (&message.actual, &message.expected) {
            self.line(&format!("   Actual   (): {actual}"));
            self.line(&format!("   Expected (): {expected}"));
        }
        if let (Some(file), Some(line)) = (&message.file, message.line) {
            self.line(&format!("   Loc: [{file}({line})]"));
        }
        let Some(open) = &mut self.open else {
            // A failure outside any function still fails the run.
            if matches!(
                message.kind,
                MessageKind::Fail | MessageKind::UnexpectedPass
            ) {
                self.report.totals.failed += 1;
                self.report.tests.push(TestRecord {
                    case: String::new(),
                    function: "UnknownTestFunc".to_owned(),
                    tag: None,
                    outcome: Outcome::Fail,
                    messages: vec![message],
                });
            }
            return;
        };
        match message.kind {
            MessageKind::Fail | MessageKind::UnexpectedPass => open.outcome = Outcome::Fail,
            MessageKind::Skip if open.outcome == Outcome::Pass => open.outcome = Outcome::Skip,
            _ => {}
        }
        open.messages.push(message);
    }

    /// The running function is over: `PASS` unless something failed or
    /// was skipped, and one more in the totals either way.
    pub(crate) fn end(&mut self) -> Outcome {
        let Some(record) = self.open.take() else {
            return Outcome::Pass;
        };
        let outcome = record.outcome;
        match outcome {
            Outcome::Pass => {
                self.line(&format!("PASS   : {}", record.identity()));
                self.report.totals.passed += 1;
            }
            Outcome::Fail => self.report.totals.failed += 1,
            Outcome::Skip => self.report.totals.skipped += 1,
        }
        self.report.tests.push(record);
        outcome
    }

    /// The running function is not one after all (it was only asked for
    /// its data): what it said stays said, and it is not counted.
    pub(crate) fn discard(&mut self) {
        self.open = None;
    }

    /// A failure that belongs to no function that ran: a file that could
    /// not be compiled, a test case that never became ready.
    pub(crate) fn failure(&mut self, case: &str, function: &str, message: Message) {
        self.begin(case, function, None);
        self.say(message);
        self.end();
    }

    /// The `Totals:` line, and the results. Nothing is printed when
    /// nothing was reported.
    pub(crate) fn finish(&mut self) -> Report {
        if let Some(started) = self.started {
            let t = self.report.totals;
            let ms = started.elapsed().as_millis();
            self.line(&format!(
                "Totals: {} passed, {} failed, {} skipped, 0 blacklisted, {ms}ms",
                t.passed, t.failed, t.skipped
            ));
            let end = format!("********* Finished testing of {} *********", self.program);
            self.line(&end);
        }
        self.started = None;
        std::mem::take(&mut self.report)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn reporter() -> (Reporter, Rc<RefCell<Vec<String>>>) {
        let lines = Rc::new(RefCell::new(Vec::new()));
        let out = lines.clone();
        let reporter = Reporter::new(
            "tests",
            Box::new(move |l| out.borrow_mut().push(l.to_owned())),
        );
        (reporter, lines)
    }

    /// The lines without the banner, the configuration and the time.
    fn body(lines: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
        lines
            .borrow()
            .iter()
            .filter(|l| !l.starts_with("*********") && !l.starts_with("Config: "))
            .map(|l| match l.split_once(" skipped") {
                Some((head, _)) if l.starts_with("Totals:") => format!("{head} skipped"),
                _ => l.clone(),
            })
            .collect()
    }

    #[test]
    fn a_run_that_reports_nothing_prints_nothing() {
        let (mut reporter, lines) = reporter();
        let report = reporter.finish();
        assert!(lines.borrow().is_empty());
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn a_function_that_says_nothing_passes() {
        let (mut reporter, lines) = reporter();
        reporter.begin("Case", "test_it", None);
        assert_eq!(reporter.end(), Outcome::Pass);
        let report = reporter.finish();
        assert_eq!(
            body(&lines),
            [
                "PASS   : Case::test_it()",
                "Totals: 1 passed, 0 failed, 0 skipped"
            ]
        );
        assert!(lines.borrow()[0].starts_with("********* Start testing of tests"));
        assert_eq!(report.totals.passed, 1);
    }

    #[test]
    fn a_failure_prints_its_sides_and_its_place_and_no_pass() {
        let (mut reporter, lines) = reporter();
        reporter.begin("Case", "test_it", Some("row 1"));
        reporter.say(
            Message::new(MessageKind::Fail, "Compared values are not the same")
                .sides("1", "2")
                .at(Some(("/dir/tst_case.qml".to_owned(), 7))),
        );
        assert_eq!(reporter.end(), Outcome::Fail);
        let report = reporter.finish();
        assert_eq!(
            body(&lines),
            [
                "FAIL!  : Case::test_it(row 1) Compared values are not the same",
                "   Actual   (): 1",
                "   Expected (): 2",
                "   Loc: [/dir/tst_case.qml(7)]",
                "Totals: 0 passed, 1 failed, 0 skipped",
            ]
        );
        assert_eq!(report.exit_code(), 1);
        let json = report.to_json();
        assert_eq!(json["tests"][0]["tag"], "row 1");
        assert_eq!(json["tests"][0]["outcome"], "fail");
        assert_eq!(json["tests"][0]["messages"][0]["line"], 7);
        assert_eq!(json["tests"][0]["messages"][0]["actual"], "1");
    }

    #[test]
    fn an_expected_failure_still_passes_and_a_skip_is_counted_apart() {
        let (mut reporter, lines) = reporter();
        reporter.begin("", "test_a", None);
        reporter.say(Message::new(MessageKind::ExpectedFail, "known"));
        reporter.end();
        reporter.begin("", "test_b", None);
        reporter.say(Message::new(MessageKind::Skip, "later"));
        reporter.end();
        let report = reporter.finish();
        assert_eq!(
            body(&lines),
            [
                "XFAIL  : test_a() known",
                "PASS   : test_a()",
                "SKIP   : test_b() later",
                "Totals: 1 passed, 0 failed, 1 skipped",
            ]
        );
        assert_eq!(report.totals.skipped, 1);
        assert!(report.to_json()["tests"][0].get("tag").is_none());
    }

    #[test]
    fn what_is_said_outside_a_function_has_qts_name_for_it() {
        let (mut reporter, lines) = reporter();
        reporter.say(Message::new(MessageKind::Debug, "qml: early"));
        reporter.finish();
        assert_eq!(body(&lines)[0], "QDEBUG : UnknownTestFunc() qml: early");
    }
}
