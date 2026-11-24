# ADR 0005: Ljos.Test, tests in TypeScript that drive the app as an agent does

- **Status:** accepted, 2026-11-24. Built: one TypeScript file,
  queries and acts, virtual-clock waits, and from running and reporting
  TAP, `--junit`, `--json` and `--filter`, and pictures (§4)
  (`features/tooling/ljos-test.feature`). The types are
  `crates/ljos_test/src/script/ljos-test.d.ts`. Not yet built: the
  `test_run` MCP tool, `mise run test:ljos` and `--attach`.
- **Terms:** [DIALECT.md](../DIALECT.md) §4.4 (TypeScript modules);
  ARCHITECTURE.md §17 and §18.1 (testing, the QtTest runner) and §19.4
  (the MCP inspector, locators); `crates/ljos_drive` (the scene driver).

## Context

An application is tested today in one of two ways:

- **QtTest** (`tst_*.qml`, `TestCase`): QML test code runs *inside* the
  application's engine and calls `mouseClick(item, x, y)` on objects it
  holds. It exists so that suites written for Qt keep running
  (ARCHITECTURE.md §18.1).
- **Gherkin** (`features/`, `ljos_bdd`): Ljós's own behaviour, in English
  steps a Rust world interprets. It is how the toolkit is specified, not
  how an application author tests an application.

Neither is what Ljós's API asks for: scripts in TypeScript (DIALECT.md),
TC39 syntax exactly, and an app seen the way an agent sees it, through
the accessibility tree and the MCP inspector's locators
(`role=button[name="Save"]`). The site's testing pages teach
`import QtTest`.

## Decision

**Ljos.Test** is a TypeScript module, `ljos:test`, whose tests run in a
JavaScript context of their own, *outside* the application, and drive it
through the MCP inspector's tools in process.

```ts
import { test, expect } from "ljos:test";

test.use({ app: "./Counter.qml", width: 240, height: 120 });

test("a click counts", async (app) => {
  await app.click('role=button[name="Add"]');
  await expect(app.find("#count")).toHaveText("1");
});
```

### 1. The API (TC39 syntax, typed by `ljos-test.d.ts`)

- `test(name, fn)`, `test.skip`, `test.only`, `describe(name, fn)`.
  `fn` is `async (app) => …`; `app` is a fresh application per test.
- `test.use({ app, width, height })` names the file's application, once,
  at the top level; `launch(path, options)` starts another one in a test
  (a second window, a second app).
- `App`: `find(locator)` gives a `Locator`; `click`, `type`, `press`,
  `scroll`, `advance(ms)`, `resize(w, h)`, `snapshot()` (the outline an
  agent reads), `logs()`, `call(tool, args)` for any other inspector
  tool.
- `Locator`: `click`, `type`, `press`, `scroll`, `property(name)`,
  `text()`, `count()`, `exists()`, `inspect()`, `nth(i)`, `find(inner)`.
  A locator is text in the inspector's language (§19.4), so a test, an
  agent and `ljos observe` name an item the same way. A locator
  describes; each act finds the item again.
- `expect(value)`: `toBe`, `toEqual`, `toBeTruthy`, `toBeFalsy`,
  `toContain`, `toMatch`, `toBeGreaterThan`, `toBeLessThan`,
  `toBeCloseTo`, `toThrow`, each with `.not`.
- `expect(locator)` retries until it holds: `toHaveText`,
  `toHaveProperty`, `toHaveCount`, `toExist`, `toBeVisible`,
  `toBeEnabled`, `toBeChecked`, `toBeFocused`. `app.waitFor(fn)` and
  `expect.poll(fn)` retry any condition, and retry when it throws.
  DECISION: an item that is not there holds under `.not` (it has no
  text, is not shown); any other failed look (more than one item, a
  refused tool) is retried and fails at the timeout, `.not` or not, so a
  negated check never passes for want of looking.
- `expect(app)` and `expect(locator)`: `toMatchPicture(name, options)`
  (§4).

DECISION: the shape is the one TypeScript test authors already know
(`node:test`, Vitest, Playwright's `expect(locator)`), so `tsc` and an
editor type a test file with one `.d.ts` and nothing is new but the
locators. No decorators, no globals: everything comes from the import.

### 2. Where a test runs

- The test file is a TypeScript module like any other (DIALECT.md §4.4):
  its types are erased by the compiler's eraser, line for line, and it
  may import its own `.ts` helpers beside it. `ljos:test` is the one bare
  name a test file may import.
- It runs in a QuickJS context of its own (`ljos_test::script`), never in
  the application's engine. The application is the one `ljos run` would
  load, on a `ljos_drive::Stage` with a virtual clock, and the test acts
  on it only through the inspector's tools (`app_find`, `app_click`,
  `app_type`, `app_press`, `app_advance`, `app_screenshot`, …) over the
  in-process MCP client (`LocalClient`), as the scenario runner does.
- DECISION: outside, through MCP, rather than inside the engine as
  `TestCase` is. A test then cannot reach what a user or an agent could
  not (no private object, no handler called directly), the same refusals
  hold (a disabled button is not clicked), there is one driver for
  tests, scenarios and agents, and the same test file can later be run
  against an app started elsewhere (`ljos test --attach`) with no change.
- The runner is backend neutral: `ljos_test::script` is given an
  `TestHost` that launches an app and answers tool calls. `ljos test`
  gives it the in-process host; it links the compiler, the runtime crate
  does not (CLAUDE.md rule 5).

### 3. Time

- Time is virtual and moves only when the test lets it: `app.advance(ms)`,
  or a retrying `expect`/`waitFor`, which checks once at once and then
  advances the app's clock 16 ms (one frame) at a time up to its timeout
  (default 5000 ms of app time, `{ timeout }` to change it). Nothing
  sleeps, so a test gives the same result on every machine and runs as
  fast as its own work.
- Every act settles the scene before it answers (ljos_drive's rules), so
  `await app.click(…)` returns with the click's frames made.
- The test's own script has a wall-clock guard (an interrupt after 60 s
  of a test) so a test that loops for ever fails rather than hangs. It
  is the only wall clock in a run; with nothing to run the runner is not
  running.

### 4. Pictures

- `await expect(app).toMatchPicture("name.png")` (or of one item,
  `expect(locator)`) takes the inspector's PNG (`app_screenshot`, the
  scene as it is after the act before it settled) and compares it with
  the file in `__pictures__/` beside the test, pixel by pixel: a pixel
  differs when a channel is more than `tolerance` (2) from the kept
  one's, and at most `maxDiffPixels` (0) may. It is not retried: an act
  has settled the scene, so a second look would see the same.
- DECISION: a missing picture is written and the test fails, saying so,
  as a new reference is something a person looks at once; the next run
  holds the test to it. A picture that differs fails and is written
  beside the kept one as `name.actual.png`;
  `ljos test --update-pictures` keeps every picture a run takes instead.
- The name is a file name and nothing more, so a test cannot write
  outside its `__pictures__/`.

### 5. Running and reporting

- `ljos test [paths…]` runs every `*.test.ts` below the paths and every
  `tst_*.qml` as before. `mise run test:ljos` is the task an application
  copies.
- Output is TAP version 14 on standard output (one `ok`/`not ok` line per
  test, a YAML block with the message and the file:line of a failure);
  `--junit FILE` writes JUnit XML for CI, `--json FILE` the JSON report.
  The exit status is the number of failed tests, as for `tst_*.qml`.
- `--filter TEXT` runs the tests whose full name (`describe › test`)
  contains the text.
- The authoring MCP server gets a `test_run` tool that runs `ljos test`
  on paths and answers the JSON report, so an agent that changed an app
  runs its tests without a shell.

### 6. QtTest

DECISION: `TestCase` stays as the compat path, unchanged: `tst_*.qml`
runs on the QtTest runner, and Ljos.Test is not built on it, nor it on
Ljos.Test. Mapping `TestCase` onto Ljos.Test would be lossy (a TestCase
holds objects and calls their functions, which a test outside the app
cannot), and the suites it serves are Qt's, held to `qmltestrunner` by
`qttest:parity`. `ljos migrate` does not rewrite test files. The site
teaches Ljos.Test and mentions QtTest only as compat.

## Slices

1. One `.test.ts` file against one app: `test`, `expect` on values, the
   TAP report, the exit status.
2. Locators and acts: `find`, `click`, `type`, `press`, `property`,
   `text`, `count`.
3. Waits on the virtual clock: `advance`, retrying `expect(locator)`,
   `waitFor`, `expect.poll`.
4. Pictures: `picture()`, `toMatchPicture`, `--update-pictures`.
5. `--junit`, `--json`, `--filter`, `describe`, `test.only`, the
   `test_run` MCP tool, `--attach`.

## Consequences

- One driver: anything an agent can do to an app, a test can, and a new
  inspector tool is a new test act without new runner code.
- A test reads what is exposed (accessibility, properties the inspector
  shows); an item a test cannot name needs an `objectName`, an `id` or
  an accessible name, which an app should give it anyway.
- Each test starts its app afresh, which costs a compile-free engine
  start per test (the project is compiled once per run).
