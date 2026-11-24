//! The runner against a host that is not an application: a counter whose
//! one button a click adds to, and a clock that only `app_advance` moves.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use ljos_render::RgbaImage;

use serde_json::{Value as Json, json};

use super::{
    LoadError, Module, Options, Outcome, Tap, TestHost, TestResult, junit, run_file, to_json,
};

#[derive(Default)]
struct Fake {
    files: HashMap<String, String>,
    apps: Vec<(u32, f64)>,
    launched: usize,
    /// Where pictures are kept; none unless a test gives one.
    pictures: Option<PathBuf>,
}

impl TestHost for Fake {
    fn load(&mut self, key: &str) -> Result<Module, LoadError> {
        let Some(text) = self.files.get(key) else {
            return Err(LoadError {
                key: key.into(),
                line: 1,
                column: 1,
                message: format!("there is no {key}"),
            });
        };
        ljos_compiler::standalone_module(key, text, &[super::MODULE])
            .map(|m| Module {
                source: m.source,
                imports: m.imports.into_iter().map(|i| i.key).collect(),
            })
            .map_err(|faults| LoadError {
                key: key.into(),
                line: faults[0].line,
                column: faults[0].column,
                message: faults[0].message.clone(),
            })
    }

    fn launch(
        &mut self,
        _: &str,
        app: &str,
        _: Option<f64>,
        _: Option<f64>,
    ) -> Result<u32, String> {
        if app != "./Counter.qml" {
            return Err(format!("there is no {app}"));
        }
        self.launched += 1;
        self.apps.push((0, 0.0));
        Ok(u32::try_from(self.apps.len() - 1).unwrap())
    }

    fn call(&mut self, app: u32, tool: &str, args: Json) -> Result<Json, String> {
        let (count, now) = self.apps.get_mut(app as usize).ok_or("no app")?;
        // The count shows once a second has passed, as a timer would.
        let shown = if *now >= 1000.0 { *count } else { 0 };
        match tool {
            "app_click" if args["target"] == "Add" => *count += 1,
            "app_click" => return Err(format!("`{}` matches nothing", args["target"])),
            "app_advance" => *now += args["ms"].as_f64().unwrap_or(0.0),
            "app_find" => {
                let total = found(&args["locator"]);
                return Ok(json!({ "structured": { "total": total }, "text": "" }));
            }
            // As the inspector does: one item, or a refusal that says why.
            "app_inspect" => {
                let target = args["target"].as_str().unwrap_or_default();
                match found(&args["target"]) {
                    0 => return Err(format!("nothing matches `{target}`")),
                    1 => {}
                    n => {
                        return Err(format!(
                            "`{target}` matches {n} items and `app_inspect` needs exactly one"
                        ));
                    }
                }
                return Ok(json!({ "structured": {
                    "properties": [{ "name": "text", "type": "string", "value": shown.to_string() }],
                    "state": { "visible": true, "enabled": true },
                }, "text": "" }));
            }
            _ => return Err(format!("no {tool}")),
        }
        Ok(json!({ "structured": {}, "text": "" }))
    }

    fn now(&mut self, app: u32) -> Result<f64, String> {
        self.apps
            .get(app as usize)
            .map(|a| a.1)
            .ok_or("no app".into())
    }

    fn picture(&mut self, app: u32, target: Option<&str>) -> Result<Vec<u8>, String> {
        let (count, _) = self.apps.get(app as usize).ok_or("no app")?;
        if target.is_some_and(|t| t != "#count") {
            return Err("nothing to picture".into());
        }
        // One pixel for each click so far, red, on a black row of four.
        let mut data = Vec::new();
        for i in 0..4 {
            data.extend(if i < *count {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 255]
            });
        }
        let image = RgbaImage {
            width: 4,
            height: 1,
            data,
        };
        image.to_png().map_err(|e| e.to_string())
    }

    fn picture_path(&self, _: &str, name: &str) -> PathBuf {
        self.pictures
            .as_ref()
            .expect("a test that compares pictures gives a directory")
            .join(super::PICTURES)
            .join(name)
    }

    fn close_all(&mut self) {
        self.apps.clear();
    }
}

/// How many items of the counter a locator finds: its count, and its
/// two buttons.
fn found(locator: &Json) -> u32 {
    match locator.as_str() {
        Some("#count") => 1,
        Some("role=button") => 2,
        _ => 0,
    }
}

fn run(files: &[(&str, &str)], options: &Options) -> (Vec<TestResult>, Rc<RefCell<Fake>>) {
    run_in(files, options, None)
}

fn run_in(
    files: &[(&str, &str)],
    options: &Options,
    pictures: Option<&Path>,
) -> (Vec<TestResult>, Rc<RefCell<Fake>>) {
    let fake = Rc::new(RefCell::new(Fake {
        pictures: pictures.map(Path::to_owned),
        files: files
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        ..Fake::default()
    }));
    let host: Rc<RefCell<dyn TestHost>> = fake.clone();
    let mut results = Vec::new();
    run_file(&host, files[0].0, options, &mut |r| results.push(r.clone()));
    (results, fake)
}

fn outcomes(files: &[(&str, &str)]) -> Vec<Outcome> {
    run(files, &Options::default())
        .0
        .into_iter()
        .map(|r| r.outcome)
        .collect()
}

fn fail(message: &str, at: &str) -> Outcome {
    Outcome::Fail {
        message: message.into(),
        at: Some(at.into()),
    }
}

const HEAD: &str =
    "import { test, expect } from \"ljos:test\";\ntest.use({ app: \"./Counter.qml\" });\n";

#[test]
fn a_value_expectation_fails_at_the_test_line() {
    let src = format!("{HEAD}test(\"one\", async () => {{\n  expect(1 + 1).toBe(3);\n}});\n");
    assert_eq!(
        outcomes(&[("a.test.ts", &src)]),
        [fail("expected 2 to be 3", "a.test.ts:4")]
    );
}

#[test]
fn types_are_erased() {
    let src = format!(
        "{HEAD}test(\"typed\", async () => {{\n  const n: number = 2;\n  expect(n).toBe(2);\n}});\n"
    );
    assert_eq!(outcomes(&[("a.test.ts", &src)]), [Outcome::Pass]);
}

#[test]
fn a_retried_expectation_lets_app_time_pass() {
    let src = format!(
        "{HEAD}test(\"later\", async (app) => {{\n  await app.click(\"Add\");\n  await expect(app.find(\"#count\")).toHaveText(\"1\");\n  expect(await app.now()).toBe(1008);\n}});\n"
    );
    assert_eq!(outcomes(&[("a.test.ts", &src)]), [Outcome::Pass]);
}

#[test]
fn a_retried_expectation_gives_up_at_its_timeout() {
    let src = format!(
        "{HEAD}test(\"soon\", async (app) => {{\n  await app.click(\"Add\");\n  await expect(app.find(\"#count\")).toHaveText(\"1\", {{ timeout: 500 }});\n}});\n"
    );
    assert_eq!(
        outcomes(&[("a.test.ts", &src)]),
        [fail(
            "expected #count to have text \"1\"; it was \"0\" after 500 ms",
            "a.test.ts:4"
        )]
    );
}

#[test]
fn a_negated_check_that_cannot_look_fails() {
    let src = format!(
        "{HEAD}test(\"ambiguous\", async (app) => {{\n  await expect(app.find(\"role=button\")).not.toBeEnabled({{ timeout: 100 }});\n}});\n"
    );
    assert_eq!(
        outcomes(&[("a.test.ts", &src)]),
        [fail(
            "expected role=button not to be enabled; it could not be looked at after 100 ms: `role=button` matches 2 items and `app_inspect` needs exactly one",
            "a.test.ts:4"
        )]
    );
}

#[test]
fn an_item_that_is_not_there_holds_only_under_not() {
    let src = format!(
        "{HEAD}test(\"gone\", async (app) => {{\n  await expect(app.find(\"#gone\")).not.toBeVisible();\n  await expect(app.find(\"#gone\")).not.toHaveText(\"0\");\n}});\ntest(\"there\", async (app) => {{\n  await expect(app.find(\"#gone\")).toHaveText(\"0\", {{ timeout: 0 }});\n}});\n"
    );
    assert_eq!(
        outcomes(&[("a.test.ts", &src)]),
        [
            Outcome::Pass,
            fail(
                "expected #gone to have text \"0\"; it was not there",
                "a.test.ts:8"
            )
        ]
    );
}

#[test]
fn a_property_an_item_does_not_have_holds_only_under_not() {
    let src = format!(
        "{HEAD}test(\"none\", async (app) => {{\n  await expect(app.find(\"#count\")).not.toHaveProperty(\"checked\", {{ timeout: 0 }});\n  await expect(app.find(\"#count\")).not.toBeChecked({{ timeout: 0 }});\n}});\ntest(\"some\", async (app) => {{\n  await expect(app.find(\"#count\")).toHaveProperty(\"checked\", {{ timeout: 0 }});\n}});\n"
    );
    assert_eq!(
        outcomes(&[("a.test.ts", &src)]),
        [
            Outcome::Pass,
            fail(
                "expected #count to have checked; it was an item with no checked",
                "a.test.ts:8"
            )
        ]
    );
}

#[test]
fn a_poll_retries_what_throws() {
    let src = format!(
        "{HEAD}test(\"late\", async (app) => {{\n  await expect.poll(async () => {{\n    if ((await app.now()) < 1000) throw new Error(\"not yet\");\n    return 1;\n  }}).toBe(1);\n}});\ntest(\"never\", async () => {{\n  await expect.poll(() => {{ throw new Error(\"never\"); }}, {{ timeout: 32 }}).toBe(1);\n}});\n"
    );
    assert_eq!(
        outcomes(&[("a.test.ts", &src)]),
        [
            Outcome::Pass,
            fail(
                "the poll did not give what was expected after 32 ms; it last threw Error: never",
                "a.test.ts:10"
            )
        ]
    );
}

#[test]
fn a_poll_moves_the_clock_of_an_application_the_test_launched() {
    let src = "import { test, expect, launch } from \"ljos:test\";\ntest(\"own\", async () => {\n  const app = await launch(\"./Counter.qml\");\n  await expect.poll(() => app.now()).toBeGreaterThan(100);\n});\n";
    assert_eq!(outcomes(&[("a.test.ts", src)]), [Outcome::Pass]);
}

#[test]
fn a_refused_act_fails_with_the_refusal() {
    let src =
        format!("{HEAD}test(\"gone\", async (app) => {{\n  await app.click(\"Gone\");\n}});\n");
    assert_eq!(
        outcomes(&[("a.test.ts", &src)]),
        [fail("`\"Gone\"` matches nothing", "a.test.ts:4")]
    );
}

#[test]
fn every_test_has_an_app_of_its_own() {
    let src = format!(
        "{HEAD}for (const n of [1, 2]) test(`t${{n}}`, async (app) => {{\n  await app.advance(1000);\n  await app.click(\"Add\");\n  expect(await app.find(\"#count\").text()).toBe(\"1\");\n}});\n"
    );
    let (results, fake) = run(&[("a.test.ts", &src)], &Options::default());
    assert!(
        results.iter().all(|r| r.outcome == Outcome::Pass),
        "{results:?}"
    );
    assert_eq!(fake.borrow().launched, 2);
    assert!(fake.borrow().apps.is_empty());
}

#[test]
fn a_helper_beside_the_test_is_imported() {
    let helper = "export function twice(n: number): number {\n  return n * 2;\n}\n";
    let src = format!(
        "{HEAD}import {{ twice }} from \"./help.ts\";\ntest(\"twice\", async () => {{\n  expect(twice(2)).toBe(4);\n}});\n"
    );
    assert_eq!(
        outcomes(&[("a.test.ts", &src), ("help.ts", helper)]),
        [Outcome::Pass]
    );
}

#[test]
fn a_file_that_does_not_load_is_one_failure_at_its_line() {
    let src = "import { test } from \"vitest\";\ntest(\"never\", () => {});\n";
    let results = outcomes(&[("a.test.ts", src)]);
    let [Outcome::Fail { message, at }] = results.as_slice() else {
        panic!("{results:?}");
    };
    assert!(message.contains("vitest"), "{message}");
    assert!(
        at.as_deref().is_some_and(|a| a.starts_with("a.test.ts:1:")),
        "{at:?}"
    );
}

#[test]
fn skip_only_and_describe() {
    let src = format!(
        "{HEAD}import {{ describe }} from \"ljos:test\";\ndescribe(\"group\", () => {{\n  test.only(\"this\", async () => {{}});\n  test(\"not this\", async () => {{ throw new Error(\"ran\"); }});\n}});\n"
    );
    let (results, _) = run(&[("a.test.ts", &src)], &Options::default());
    let named: Vec<(String, Outcome)> = results
        .into_iter()
        .map(|r| (r.full_name(), r.outcome))
        .collect();
    assert_eq!(
        named,
        [
            ("a.test.ts > group > this".into(), Outcome::Pass),
            ("a.test.ts > group > not this".into(), Outcome::Skip),
        ]
    );
}

#[test]
fn a_test_that_never_settles_fails() {
    let src = format!("{HEAD}test(\"hangs\", () => new Promise(() => {{}}));\n");
    let results = outcomes(&[("a.test.ts", &src)]);
    assert!(
        matches!(&results[..], [Outcome::Fail { message, .. }] if message.contains("did not finish")),
        "{results:?}"
    );
}

#[test]
fn a_test_that_loops_is_stopped() {
    let src = format!("{HEAD}test(\"loops\", async () => {{\n  for (;;) {{}}\n}});\n");
    let options = Options {
        script_limit: Duration::from_millis(50),
        ..Options::default()
    };
    let (results, _) = run(&[("a.test.ts", &src)], &options);
    assert!(
        matches!(&results[0].outcome, Outcome::Fail { message, .. } if message.contains("stopped")),
        "{results:?}"
    );
}

#[test]
fn the_tap_report() {
    let src = format!(
        "{HEAD}test(\"ok\", async () => {{}});\ntest(\"bad\", async () => {{\n  expect(\"it's\").toBe(\"x\");\n}});\ntest.skip(\"later\", async () => {{}});\n"
    );
    let (results, _) = run(&[("a.test.ts", &src)], &Options::default());
    let mut tap = Tap::new();
    let mut lines = vec![tap.header().to_string()];
    for r in &results {
        lines.extend(tap.result(r));
    }
    lines.extend(tap.footer());
    assert_eq!(
        lines.join("\n"),
        "TAP version 14\n\
         ok 1 - a.test.ts > ok\n\
         not ok 2 - a.test.ts > bad\n  \
         ---\n  \
         message: 'expected \"it''s\" to be \"x\"'\n  \
         at: a.test.ts:5\n  \
         ...\n\
         ok 3 - a.test.ts > later # SKIP\n\
         1..3\n# pass 1\n# fail 1\n# skip 1"
    );
    assert_eq!(
        to_json(&results)["totals"],
        json!({ "passed": 1, "failed": 1, "skipped": 1 })
    );
}

fn clicks_then_picture(clicks: usize, matcher: &str) -> String {
    format!(
        "{HEAD}test(\"looks\", async (app) => {{\n  for (let i = 0; i < {clicks}; i++) await app.click(\"Add\");\n  await expect(app).toMatchPicture({matcher});\n}});\n"
    )
}

fn failure(results: &[TestResult]) -> (String, Option<String>) {
    match &results[0].outcome {
        Outcome::Fail { message, at } => (message.clone(), at.clone()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_missing_picture_is_written_and_the_test_fails_once() {
    let dir = tempfile::tempdir().unwrap();
    let src = clicks_then_picture(1, "\"one.png\"");
    let files = [("a.test.ts", src.as_str())];
    let (first, _) = run_in(&files, &Options::default(), Some(dir.path()));
    let (message, at) = failure(&first);
    assert_eq!(
        message,
        "there was no __pictures__/one.png; this run's picture is written there: look at it, and run again"
    );
    assert_eq!(at.as_deref(), Some("a.test.ts:5"));
    assert!(dir.path().join("__pictures__/one.png").is_file());
    let (second, _) = run_in(&files, &Options::default(), Some(dir.path()));
    assert_eq!(second[0].outcome, Outcome::Pass);
}

#[test]
fn a_picture_that_differs_fails_and_keeps_what_it_saw() {
    let dir = tempfile::tempdir().unwrap();
    let one = clicks_then_picture(1, "\"p.png\"");
    let two = clicks_then_picture(2, "\"p.png\"");
    let _ = run_in(
        &[("a.test.ts", &one)],
        &Options::default(),
        Some(dir.path()),
    );
    let (results, _) = run_in(
        &[("a.test.ts", &two)],
        &Options::default(),
        Some(dir.path()),
    );
    let (message, _) = failure(&results);
    assert_eq!(
        message,
        "the picture differs from __pictures__/p.png in 1 pixel (at most 0 may). What this run saw \
         is in __pictures__/p.actual.png; `ljos test --update-pictures` keeps it"
    );
    assert!(dir.path().join("__pictures__/p.actual.png").is_file());

    // Allowed, it passes, and what the failure left is gone.
    let allowing = clicks_then_picture(2, "\"p.png\", { maxDiffPixels: 1 }");
    let (results, _) = run_in(
        &[("a.test.ts", &allowing)],
        &Options::default(),
        Some(dir.path()),
    );
    assert_eq!(results[0].outcome, Outcome::Pass);
    assert!(!dir.path().join("__pictures__/p.actual.png").exists());
}

#[test]
fn updating_pictures_keeps_this_runs() {
    let dir = tempfile::tempdir().unwrap();
    let one = clicks_then_picture(1, "\"p.png\"");
    let two = clicks_then_picture(2, "\"p.png\"");
    let _ = run_in(
        &[("a.test.ts", &one)],
        &Options::default(),
        Some(dir.path()),
    );
    let update = Options {
        update_pictures: true,
        ..Options::default()
    };
    let (results, _) = run_in(&[("a.test.ts", &two)], &update, Some(dir.path()));
    assert_eq!(results[0].outcome, Outcome::Pass);
    let (results, _) = run_in(
        &[("a.test.ts", &two)],
        &Options::default(),
        Some(dir.path()),
    );
    assert_eq!(results[0].outcome, Outcome::Pass);
}

#[test]
fn a_picture_is_named_by_a_png_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let src = clicks_then_picture(0, "\"../p.png\"");
    let (results, _) = run_in(
        &[("a.test.ts", &src)],
        &Options::default(),
        Some(dir.path()),
    );
    let (message, _) = failure(&results);
    assert!(
        message.contains("with no directory: not \"../p.png\""),
        "{message}"
    );
}

#[test]
fn a_picture_is_not_named_as_what_a_failure_keeps() {
    let dir = tempfile::tempdir().unwrap();
    let src = clicks_then_picture(0, "\"p.actual.png\"");
    let (results, _) = run_in(
        &[("a.test.ts", &src)],
        &Options::default(),
        Some(dir.path()),
    );
    let (message, _) = failure(&results);
    assert!(message.contains("not `.actual.png`"), "{message}");
    assert!(!dir.path().join("__pictures__/p.actual.png").exists());
}

#[test]
fn a_reference_that_cannot_be_read_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the reference would be: there, but not readable.
    std::fs::create_dir_all(dir.path().join("__pictures__/p.png")).unwrap();
    let src = clicks_then_picture(0, "\"p.png\"");
    let (results, _) = run_in(
        &[("a.test.ts", &src)],
        &Options::default(),
        Some(dir.path()),
    );
    let (message, _) = failure(&results);
    assert!(
        message.starts_with("__pictures__/p.png cannot be read: "),
        "{message}"
    );
    assert!(dir.path().join("__pictures__/p.png").is_dir());
}

#[test]
fn a_file_that_fails_to_load_ends_what_it_started() {
    let src = "import { launch } from \"ljos:test\";\nlaunch(\"./Counter.qml\");\nthrow new Error(\"after\");\n";
    let (results, fake) = run(&[("a.test.ts", src)], &Options::default());
    assert!(
        matches!(&results[..], [TestResult { outcome: Outcome::Fail { message, .. }, .. }] if message.contains("after")),
        "{results:?}"
    );
    assert_eq!(fake.borrow().launched, 1);
    assert!(fake.borrow().apps.is_empty());
}

#[test]
fn a_file_whose_tests_cannot_be_listed_fails() {
    let src = format!(
        "{HEAD}test(\"one\", async () => {{}});\nglobalThis.__ljos_test_list = () => {{ throw new Error(\"no list\"); }};\n"
    );
    let results = outcomes(&[("a.test.ts", &src)]);
    let [Outcome::Fail { message, .. }] = results.as_slice() else {
        panic!("{results:?}");
    };
    assert!(
        message.starts_with("the tests of a.test.ts cannot be listed: ")
            && message.contains("no list"),
        "{message}"
    );
}

#[test]
fn a_group_is_named_by_a_string_and_adds_its_tests_at_once() {
    let unnamed =
        format!("{HEAD}import {{ describe }} from \"ljos:test\";\ndescribe(1, () => {{}});\n");
    let results = outcomes(&[("a.test.ts", &unnamed)]);
    assert!(
        matches!(&results[..], [Outcome::Fail { message, .. }] if message.contains("a group is named by a string")),
        "{results:?}"
    );
    let later = format!(
        "{HEAD}import {{ describe }} from \"ljos:test\";\ndescribe(\"g\", async () => {{\n  test(\"t\", async () => {{}});\n}});\n"
    );
    let results = outcomes(&[("a.test.ts", &later)]);
    assert!(
        matches!(&results[..], [Outcome::Fail { message, .. }] if message.contains("its function is not async")),
        "{results:?}"
    );
}

#[test]
fn arrays_of_other_lengths_are_not_equal() {
    let src = format!(
        "{HEAD}test(\"holes\", async () => {{\n  expect(new Array(1)).not.toEqual([]);\n  expect([]).not.toEqual(new Array(1));\n}});\n"
    );
    assert_eq!(outcomes(&[("a.test.ts", &src)]), [Outcome::Pass]);
}

#[test]
fn a_global_pattern_matches_every_time_it_is_used() {
    let src = format!(
        "{HEAD}test(\"again\", async () => {{\n  const re = /a/g;\n  expect(\"a\").toMatch(re);\n  expect(\"a\").toMatch(re);\n  expect(() => {{ throw new Error(\"a\"); }}).toThrow(re);\n  expect(() => {{ throw new Error(\"a\"); }}).toThrow(re);\n}});\n"
    );
    assert_eq!(outcomes(&[("a.test.ts", &src)]), [Outcome::Pass]);
}

fn result(name: &str, outcome: Outcome, log: &[&str]) -> TestResult {
    TestResult {
        file: "a.test.ts".into(),
        name: vec![name.into()],
        outcome,
        log: log.iter().map(|l| (*l).into()).collect(),
    }
}

#[test]
fn a_tap_report_keeps_names_messages_and_logs_in_their_place() {
    let mut tap = Tap::new();
    let lines = tap.result(&result(
        "a # b \\ c\nd",
        Outcome::Fail {
            message: "one\ntwo 'x'".into(),
            at: None,
        },
        &["first\nsecond"],
    ));
    assert_eq!(
        lines,
        [
            "# first",
            "# second",
            "not ok 1 - a.test.ts > a \\# b \\\\ c d",
            "  ---",
            "  message: \"one\\ntwo 'x'\"",
            "  ...",
        ]
    );
}

#[test]
fn a_junit_report_writes_no_character_xml_forbids() {
    let report = junit(&[result(
        "bell\u{7}",
        Outcome::Fail {
            message: "nul\u{0}".into(),
            at: None,
        },
        &[],
    )]);
    assert!(!report.contains(['\u{0}', '\u{7}']), "{report}");
    assert!(report.contains("name=\"bell\u{FFFD}\""), "{report}");
    assert!(report.contains("message=\"nul\u{FFFD}\""), "{report}");
}
