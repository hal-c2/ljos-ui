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

use super::{LoadError, Module, Options, Outcome, Tap, TestHost, TestResult, run_file, to_json};

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
                let total = u32::from(args["locator"] == "#count");
                return Ok(json!({ "structured": { "total": total }, "text": "" }));
            }
            "app_inspect" => {
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
