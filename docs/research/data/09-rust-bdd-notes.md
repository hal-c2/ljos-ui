# 09 - Rust BDD (Gherkin) + egui_kittest + rquickjs Send notes

Researched 2026-10-10. Sources: crates.io API, GitHub API, docs.rs, and the actual crate sources
(cucumber main + 0.23.0 book, gherkin main, egui_kittest 0.36.2, kittest 0.4.0/0.5.0, egui 0.36.2,
eframe 0.36.2, rquickjs / rquickjs-core 0.14.0). Items marked UNVERIFIED were not confirmed.

## TL;DR / recommendation

- Use `cucumber` 0.23.0 (+ `gherkin` 0.16.0, which it re-exports as `cucumber::gherkin`). Mature, 17M downloads, shares real
  Gherkin with C++/Elixir/TS ecosystems (same upstream grammar + i18n keywords + Rules + tag expressions).
- `World` is NOT required to be `Send` (`pub trait World: Sized + 'static`); runner is `!Send`/`!Sync`, steps return
  `LocalBoxFuture`. So a World holding egui `Harness` (not Send) and/or rquickjs (non-`parallel` => !Send) is fine, as long as
  everything runs on one thread (`futures::executor::block_on` or tokio current_thread).
- Custom tag semantics (`@backlog-<surface>` excluded unless `--backlog`): cucumber's `--tags` / `--name` CLI REPLACE (do not AND with)
  the `filter_run` closure (see "Gotcha" below). Do the backlog exclusion in a wrapper `Parser` (or by pre-filtering Features) so it is independent
  of `--tags`/`--name`; add `--backlog` via custom CLI (`cli::Compose`). Fallback if that gets awkward: roll own runner on `gherkin` crate (~few hundred LOC).
- rstest-bdd rejected (13 stars, no documented tag filtering, no Rule, ISC license, 0.x churn).

---

## A) Rust Gherkin tooling

### cucumber crate (cucumber-rs/cucumber)
- Latest: **0.23.0, 2026-04-23** (crates.io). Prior: 0.22.1 (2025-12-23), 0.22.0 (2025-12-12), 0.21.1 (2024-06-16). 17.08M downloads.
- **MSRV 1.88** (since 0.22.0), edition 2024. License MIT OR Apache-2.0. Repo https://github.com/cucumber-rs/cucumber : 746 stars, 89 forks,
  `pushed_at` 2026-07-27 (GitHub API; repo `updated_at` 2026-10-08), ~32 open issues+PRs, not archived. Maintained but slow cadence (~2-3 releases/yr).
- Docs: https://docs.rs/cucumber/0.23.0 ; book https://cucumber-rs.github.io/cucumber/current/ ; source of book `book/src/*.md` in repo.
- 0.23.0 changelog: BREAKING tag-expression operator precedence now matches upstream Cucumber; gherkin bumped to 0.16; `And`/`But` may start a Scenario (treated as Given).
  0.22.0: MSRV 1.88; event types use pointer-optimized `Source` instead of `Arc`; expanded `Scenario Outline`s keep ONLY the currently executed Examples row
  (so `filter_run` sees one row); added `CUCUMBER_FILTER_TAGS` env var. 0.22.1: fixed spaces in `Examples` placeholders.
- Cargo features: `macros` (default; pulls cucumber-codegen, cucumber-expressions, inventory, anyhow), `timestamps`, `output-json` (implies timestamps),
  `output-junit` (implies timestamps), `libtest` (implies timestamps; IntelliJ), `tracing`. (Task text said `junit`/`json` - the real names are `output-junit`/`output-json`.)
- Always-on deps include `clap`, `futures`, `gherkin`, `regex`, `globwalk`, `humantime`. tokio is only a dev-dependency (NOT required).

#### Setup (verbatim from book quickstart)
```toml
[dev-dependencies]
cucumber = "0.23"
tokio = { version = "1.10", features = ["macros", "rt-multi-thread", "time"] }   # optional; futures is enough
[[test]]
name = "example"      # same as tests/example.rs
harness = false       # cucumber prints its own output instead of libtest
```
```rust
#[derive(Debug, Default, World)]
pub struct AnimalWorld { cat: Cat }
#[given(regex = r"^a (hungry|satiated) cat$")]
async fn hungry_cat(world: &mut AnimalWorld, state: String) { ... }
#[when("I feed the cat")] / #[then(expr = "the cat is {string}")]
#[tokio::main] async fn main() { AnimalWorld::run("tests/features/book/quickstart/concurrent.feature").await; }
```
Run: `cargo test --test example -- --tags '@a and not @b'`.

#### World trait / derive (docs.rs 0.23.0)
```rust
pub trait World: Sized + 'static {
    type Error: Display;
    fn new() -> impl Future<Output = Result<Self, Self::Error>>;           // required
    // provided (need Self: Debug + WorldInventory, `macros` feature):
    fn collection() -> Collection<Self>;
    fn cucumber<I: AsRef<Path>>() -> Cucumber<Self, Basic, I, Basic<Self>, Summarize<Normalize<Self, Basic>>>;
    fn run<I: AsRef<Path>>(input: I) -> impl Future<Output = ()>;
    fn filter_run<I, F>(input: I, filter: F) -> impl Future<Output = ()>
        where F: Fn(&Feature, Option<&Rule>, &Scenario) -> bool + 'static;
}
```
- `#[derive(World)]` uses `Default::default()` unless `#[world(init = path::to::fn)]`; init fn may be sync or async, infallible or `Result`.
  Fresh World per scenario (and per retry attempt).
- NO `Send` bound anywhere on World. `runner::Basic` is `!Send + !Sync` (docs.rs). `step::Step<W> = for<'a> fn(&'a mut W, Context) -> LocalBoxFuture<'a, ()>`
  (fn pointer, so no captured state in steps; share state via World or thread_local/statics). before/after hooks also `LocalBoxFuture` closures (`.boxed_local()`).
  Source check (src/runner/basic.rs): no task spawning for scenarios; concurrency is cooperative on one task via `futures` streams; only a retry-timer
  helper uses `thread::spawn` (no World passed). `catch_unwind` wraps every step (panics = failures; steps must be unwind-safe-ish, they use `AssertUnwindSafe`).
- Therefore non-Send World works: `fn main() { futures::executor::block_on(World::cucumber().run_and_exit("features")) }` (futures is already a dependency)
  or `#[tokio::main(flavor = "current_thread")]`. Even `#[tokio::main]` multi-thread works because the top-level future is driven by `block_on` on main thread
  and is not spawned (UNVERIFIED by running; derived from signatures). No LocalSet / dedicated-thread proxy needed.
- Caveat: default concurrency is up to 64 scenarios interleaved at `.await` points on the same thread. Sync steps never interleave; with non-async steps it is effectively serial.
  If World holds global-ish state (a window, GL context, a QML engine singleton), use `--concurrency 1` / `.max_concurrent_scenarios(1)` or tag `@serial`.
- Step attributes: `#[given]`, `#[when]`, `#[then]`; forms `#[given("literal")]`, `#[given(regex = "...")]`, `#[given(expr = "...")]` (Cucumber Expressions;
  regex/expr validated at compile time). Fn params: first `&mut World`; others `FromStr` (or slice of FromStr); param named `step` or marked `#[step]` receives `&gherkin::Step`.
  May return `Result<_, impl Display>`; sync or async fns both OK.
  Builtin expr types shown in book: `{word}`, `{int}`, `{string}`, optional text `time(s)`; custom: `#[derive(Parameter)] #[param(name="hungriness", regex="hungry|satiated")]` + FromStr.
- **Ambiguity**: `Collection::find` returns `AmbiguousMatchError` if >1 regex matches a step ("Every Step has to match with exactly 1 Regex"); runner turns it into
  `event::StepError::AmbiguousMatch` => step FAILS at run time (not a compile error; JSON writer reports status "ambiguous"). Missing step => Skipped.
- **Docstrings / tables** via `step: &Step` param: `step.docstring: Option<String>`, `step.table: Option<gherkin::Table>` (`table.rows: Vec<Vec<String>>`; first row is
  header: `table.rows.iter().skip(1)`). Escapes in cells: `\n`, `\|`, `\\`. Docstring delimiters `"""` or ``` with optional content-type label.
  `-vv` prints World on failed steps, `-vvv` prints docstrings.
- **Scenario Outline**: `<placeholder>` substituted at parse time (in step text, docstrings, tables); Examples tags are merged into the expanded scenario's tags
  (src/feature.rs). README known issue: `Scenario Outline` treated same as `Outline`/`Example` keyword variants (gherkin issue #19).
- **Rule**: no extra implementation work; Feature + Rule + Scenario tags are all merged for tag filtering. **Background**: supported (Feature-level and Rule-level in AST). Tags can't be placed on Background or steps.
- **Spoken languages**: `Cucumber::language("is")` or `# language: xx` header (gherkin has languages.json from upstream).
- **Tags**: `--tags/-t <tagexpr>` (env `CUCUMBER_FILTER_TAGS`), grammar from `gherkin::tagexpr::TagOperation` (`@a and (@b or not @c)`; precedence fixed to upstream in 0.23).
  Parsed `Scenario.tags` entries are stored WITHOUT the leading `@`. Special tags: `@serial` (runs alone), `@allow.skipped` (exempt from `fail_on_skipped`),
  `@retry`, `@retry(N)`, `@retry.after(1s)`, `@retry(10).after(100ms)`.
- **Filtering**: `World::filter_run(path, |feature, rule, scenario| bool)` / `Cucumber::filter_run[_and_exit]`. 
  **GOTCHA (source src/cucumber.rs `filter_run`)**: if `--name` (regex on scenario name) is given it is used INSTEAD of the closure; else if `--tags` is given it is used INSTEAD of
  the closure; only if neither is set does the closure apply. (`--tags` and `--name` also `conflicts_with` each other.) So the closure is a default filter, not an always-on one.
  Path/file filtering: `-i/--input <glob>` (glob of feature files; default `*.feature` under the run path), or just pass a single file/dir to `run()`.
- **CLI options** (`cucumber::cli::Opts`, clap): `-n/--name <regex>` (alias `--scenario-name`), `-t/--tags`, `-i/--input <glob>`, `-c/--concurrency <int>` (default 64),
  `--fail-fast`, `--retry <n>`, `--retry-after <dur>` (e.g. 12min5s), `--retry-tag-filter <tagexpr>`, `-v/-vv/-vvv`, `--color auto|always|never`, `-h`. Writers add options (e.g. libtest `--format=json`).
  Code-config is overridden by CLI. Extend with `Cucumber::with_cli(cli::Opts::<_,_,_,MyArgs>::parsed())`; `cli::Compose`, `cli::Empty`.
- **Retries**: tags above, or `.retries(n)`, `.retry_after(Duration)`, `.retry_filter(TagOperation)`, `.retry_options(Fn -> Option<RetryOptions>)`. Whole scenario re-run with a new World.
  Precedence: `@retry(3)` caps count regardless of `--retry`; `@retry.after(1s)` fixes delay; `@retry(3).after(1s)` ignores both CLI opts. With fail-fast a scenario fails only after exhausting retries.
- **Concurrency / run API** (all on `Cucumber<..., Basic<W,F,B,A>, ...>`): `max_concurrent_scenarios(impl Into<Option<usize>>)`, `fail_fast()`, `which_scenario(Fn(&Feature,Option<&Rule>,&Scenario)->ScenarioType)`
  (`ScenarioType::{Concurrent,Serial}`; default = Serial iff tagged `@serial`), `before(|feat,rule,sc,&mut world| fut.boxed_local())`, `after(...)` (one each; later call replaces),
  `steps(Collection)`, `.given/.when/.then(regex, step_fn)` for hand-registered steps. Finish: `.run(input).await -> Writer` (inspect stats yourself) or `.run_and_exit(input).await`
  (panics => nonzero exit on parse error or any failed step) / `.filter_run_and_exit`. `fail_on_skipped()` / `fail_on_skipped_with(filter)` makes undefined/skipped steps fail the run (exempt `@allow.skipped`).
  `repeat_failed()/repeat_skipped()/repeat_if(f)` re-print failures at the end.
- **Writers**: default `Summarize<Normalize<Basic>>` (terminal). `writer::JUnit::new(file, verbosity)` (feature `output-junit`), `writer::Json::new(file)` (`output-json`, Cucumber JSON),
  `writer::Libtest::or_basic()` (`libtest`; libtest-compatible JSON with `--format=json`, accurate timings only for serial), `writer::Tee` for multiple outputs:
  `writer::Basic::stdout().summarized().tee::<World,_>(writer::JUnit::for_tee(file,0)).normalized()`. Pipeline is constructed in reverse; `.normalized()` last.
  Custom Parser/Runner/Writer traits documented (book "Architecture").
- Custom parser example: `impl<I> cucumber::Parser<I> for P { type Cli; type Output = Stream<Item=parser::Result<gherkin::Feature>>; fn parse(self, input: I, cli: Self::Cli) -> Self::Output }`
  - a wrapper around `parser::Basic` that drops/rewrites scenarios is a natural place for `@backlog-*` exclusion. (Design suggestion; not compiled by me.)
- Env var: `CUCUMBER_FILTER_TAGS`. No other env documented.

### gherkin crate (cucumber-rs/gherkin)
- **0.16.0, 2026-04-23**; 0.15.0 2025-12-12; 0.14.0 2023-07-14. 17.4M downloads. MSRV 1.88. MIT OR Apache-2.0. 93 stars, pushed 2026-06-19, 8 open issues. peg-based parser.
- Features: `parser` (default), optional `serde`, `juniper`. API: `Feature::parse_path(path, GherkinEnv)`, `Feature::parse(str, GherkinEnv)`; `GherkinEnv` selects language.
  AST (all pub fields): `Feature{keyword,name,description,background,scenarios,rules,tags,span,position,path}`, `Rule{..,background,scenarios,tags}`,
  `Background{steps}`, `Scenario{keyword,name,steps,examples,tags,span,position}` (outlines are NOT expanded here; cucumber crate expands), `Examples{table,tags}`,
  `Step{keyword,ty:StepType::{Given,When,Then},value,docstring:Option<String>,table:Option<Table>,span,position}`, `Table{rows:Vec<Vec<String>>}`.
  `gherkin::tagexpr::TagOperation::{And,Or,Not,Tag}` with `FromStr` ("@a and @b") - reusable for a custom runner. `is_language_supported(name)`.
  i18n keywords from upstream gherkin-languages.json. No comments retained, no pickles/compiled-IDs (not the Messages protocol).
- Custom runner on top: ~ parse each `.feature`, filter by own tag logic, expand outlines (replace `<col>`), match step text against own registry, run on one thread. Feasible and small;
  but you'd lose writers, concurrency, retries, JUnit, cucumber expressions.

### Alternatives
| crate | latest | updated | notes |
|---|---|---|---|
| cucumber | 0.23.0 | 2026-04-23 | recommended |
| gherkin | 0.16.0 | 2026-04-23 | parser used by cucumber |
| rstest-bdd | 0.6.0 | 2026-09-14 | github.com/leynos/rstest-bdd, 13 stars, 0 forks, ISC, MSRV 1.88, built on rstest + gherkin 0.16 + separate `-macros/-harness/-patterns/-policy` crates; scenario outlines -> rstest cases; tokio/async-std harness adapters; README documents no tag filtering, no Rule (name filtering via `cargo test`). Active but tiny/single-maintainer (67k downloads) |
| cucumber_rust | 0.10.0 | 2021-10-24 | DEAD, description "Use `cucumber` crate instead!" |
| gherkin_rust | 0.10.1 | 2021-11-30 | DEAD, superseded by `gherkin` |
| behave | 0.9.2 | 2026-03-17 | NOT Gherkin: "zero-keyword DSL" with expect! matchers (4.6k dl) |
| kitten | 0.5.0 | 2023-09-04 | stale lightweight BDD (not Gherkin-compatible by my reading, UNVERIFIED) |
| litmus | 0.5.5 | 2025-09-12 | macro-free BDD harness, tiny |
| gherkin-cargo-test | 0.11.0 | 2026-08-25 | "smallest honest Gherkin runner + linter" on libtest-mimic, ~1k dl, github.com/bingh0/gherkin-cargo-test; interesting for libtest integration, tiny adoption |
| speculoos | 0.13.0 | 2025-04-10 | fluent assertions only, not BDD |
| bdd | 0.1.0 | 2019 | unrelated (data duplicator) |
| "pickles" | n/a | - | no Rust BDD crate by that name found (only `pickls` LSP, python pickle crates) |
| cucumber-reporter | 0.3.0 | 2026-03-24 | HTML reporter for cucumber-rs |

### Recommendation details (project: custom tag semantics, non-Send UI state, shared Gherkin with C++/Elixir/TS)
1. `cucumber` 0.23 + `gherkin` 0.16: standard Gherkin dialect parity with cucumber-js/cucumber-cpp/cucumber-elixir (Rules, Examples tags, docstrings, tables, i18n).
2. Non-Send: fine natively (see above). Keep `harness = false`, drive with `futures::executor::block_on`, run `-c 1` if UI state is process-global.
3. Tags: do NOT rely on `filter_run` for `@backlog-*` because `--tags`/`--name` bypass it. Options (best first):
   a. Wrapper `Parser` that strips scenarios having any tag starting `backlog-` unless a custom `--backlog` flag (via `cli::Compose`) is set; works with all other filters.
   b. Parse CLI yourself first (clap), build a single `TagOperation` string `(<user --tags>) and not (@backlog-a or @backlog-b ...)` (needs enumerated surfaces since TagOperation has no wildcards) and `with_cli(Opts{ tags_filter: Some(..), ..})`.
   c. Own runner on `gherkin` crate.
4. Report with `output-junit` via `Tee`; unmatched/ambiguous steps: use `.fail_on_skipped()` so undefined steps fail CI (ambiguity already fails at run time).

---

## B) egui_kittest

- **egui current = 0.36.2 (2026-09-08)**; 0.36.1 2026-08-07; 0.36.0 2026-08-05; 0.35.0 2026-06-25; 0.34.3 2026-05-27; cadence ~every 1-2 months. egui repo 31.1k stars, pushed 2026-10-09, 1037 open issues.
- **egui_kittest 0.36.2 (2026-09-08)**, versions track egui exactly (0.36.1 2026-08-07, 0.36.0 2026-08-05, 0.35.0 2026-06-25). 968k downloads. MIT OR Apache-2.0.
  **rust-version 1.95**, edition 2024. Depends on `kittest 0.4.0` (kittest 0.5.0 released 2026-10-05 - newer; renames `AccessKitNode` to wrap `accesskit_consumer::NodeRef`; expect egui 0.37 to use it),
  `egui/eframe 0.36.2`, `wgpu 30.0` (optional), `dify 0.8.0` (optional), `accesskit 0.24.1` (via egui).
- Features: `wgpu` (egui-wgpu renderer, headless; default adapter selector prefers CPU/software e.g. lavapipe, then discrete GPU, Metal>Vulkan>Dx12>Gl), `snapshot` (dify image diff + `image` png),
  `eframe` (`Harness::new_eframe`, `build_eframe`; enables `eframe/accesskit`), `x11`. For image snapshots enable BOTH `snapshot` and `wgpu`.
- Docs: https://docs.rs/egui_kittest ; source https://github.com/emilk/egui/tree/main/crates/egui_kittest ; kittest https://github.com/rerun-io/kittest (docs.rs/kittest).

### Harness API (verified from 0.36.2 source)
- There is **no `Harness::new`**. Constructors: `Harness::new_ui(|ui| ..)` (stateless, `Harness<'a, ()>`), `Harness::new_ui_state(|ui, &mut state| .., state)`,
  `Harness::new_eframe(|cc| App)` (feature `eframe`; `State: eframe::App + 'static`), `Harness::builder()` -> `HarnessBuilder` with
  `.with_size(impl Into<Vec2>)` (default 800x600), `.with_pixels_per_point(f32)` (default 1.0), `.with_theme(Theme)` (default Dark), `.with_os(OperatingSystem)` (default Nix),
  `.with_max_steps(u64)` (default 4), `.with_step_dt(f32)` (default 0.25), `.with_wait_for_pending_images(bool)` (default true), `.with_render_every_step(bool)`,
  `.with_options(SnapshotOptions)` (snapshot feature), `.with_render_options(..)`, `.renderer(impl TestRenderer)`, `.wgpu()` / `.wgpu_setup(setup)`; then `.build_ui(f)`, `.build_ui_state(f, state)`, `.build_eframe(f)`.
- Construction runs one frame then `run_ok()` (to show Areas and settle animations). It sets `ctx.enable_accesskit()`, disables text-cursor blink, `scroll_animation = none`, `animation_time = 0.0`.
- Methods: `step()` (one frame per queued event, or one frame if none), `run() -> u64` (loop of `step` until root viewport `repaint_delay != ZERO`; PANICS with `ExceededMaxStepsError` if > max_steps, error lists `repaint_causes`),
  `try_run() -> Result<u64, ExceededMaxStepsError>`, `run_ok() -> Option<u64>`, `try_run_realtime()` (sleeps step_dt between frames, for async image loading etc.), `run_steps(n)`,
  `fit_contents()` (sizing pass, resizes to content incl. popups/tooltips; only for ui/ui_state harnesses), `set_size`, `set_pixels_per_point`, `input()/input_mut()` (RawInput), `output()`, `state()/state_mut()/into_state()`,
  `kittest_state()`, `root() -> Node`, `ctx: egui::Context` (pub field), `mask(rect)`, `render() -> Result<RgbaImage,String>` (wgpu/snapshot), `spawn_eframe_app()` (deprecated, debugging only; macOS needs `harness=false`).
- Input injection (all queue events; applied on next `step`/`run`): `event(egui::Event)`, `event_modifiers(ev, mods)`, `key_down/key_up/key_press(egui::Key)`, `key_press_modifiers(mods, key)`,
  `key_combination(&[Key])`, `key_combination_modifiers`, `hover_at(Pos2)`, `drag_at`, `drop_at`, `remove_cursor()` (PointerGone). Note `key_press` lives on **Harness**, not Node; `Key` is `egui::Key`, modifiers are `egui::Modifiers`.
- Node (egui_kittest::Node, implements `kittest::NodeT`): `click()`, `click_secondary()`, `click_button(PointerButton)`, `click_modifiers`, `click_button_modifiers`, `click_accesskit()` (Action::Click via AccessKit, works for off-screen),
  `hover()`, `focus()`, `type_text(&str)` (emits `Event::Text`; does not focus - call `focus()`/click first), `value()`, `is_focused()`, `rect()` (logical coords), `scroll_to_me()`, `scroll_up/down/left/right()`, `accesskit_node()`.
  Events are queued on a shared `Mutex<Vec<Event>>`; you must `harness.run()`/`step()` afterwards.
- kittest `Queryable` (implemented for `Harness` and `Node`): `get_by_label`, `get_by_label_contains`, `get_by_role(accesskit::Role)`, `get_by_role_and_label(Role, &str)`, `get_by_value`, `get_by(By)`, plus `get_all_by_*`,
  `query_by_*` (-> Option), `query_all_by_*`. `get_*` panics if 0 or >1 matches. `By::new().label(..).label_contains(..).role(..).value(..).predicate(|n| ..).include_labels().recursive(bool)`.
  Note: AccessKit stores `Role::Label` text in `value`, kittest handles that; a node labelled by another node excludes the label node from label queries unless `include_labels()`.
  `NodeT`: `children()`, `children_recursive()`, `parent()`, `accesskit_node()`.
- **Time**: virtual. Harness never sets `RawInput.time`; it sets `predicted_dt = step_dt` each frame so egui advances `time += predicted_dt` (egui `input_state/mod.rs:374`). No wall clock involved in `run()`; animations
  advance by step_dt per frame (and animation_time is forced 0 by the harness style). `request_repaint()` (immediate) keeps `run()` looping until `max_steps` then panics; `request_repaint_after(d>0)` makes `run()` STOP
  (only immediate repaints count) - timers must be advanced manually with `run_steps(n)` (virtual elapsed = n * step_dt) or by `input_mut().time = Some(..)`. Spinners/continuous animation => use `step`/`run_steps`, not `run`.
- **AccessKit tree**: `harness.kittest_state()` (kittest::State holding accesskit_consumer tree), `harness.root()`, `node.accesskit_node()` gives full accesskit_consumer node (role, label, toggled, value, bounds, etc.).
- **Snapshots**: `harness.snapshot("name")`, `try_snapshot`, `snapshot_options(name, &SnapshotOptions)`, `try_snapshot_options`, free fns `image_snapshot[_options]`, `try_image_snapshot[_options]`;
  `SnapshotOptions::new().threshold(impl Into<OsThreshold<f32>>).max_failed_pixels(impl Into<OsThreshold<usize>>).output_path(..)`; `OsThreshold::new(x).windows(..).macos(..).linux(..)`.
  Defaults: threshold 0.6 (per-pixel weighted squared YIQ distance, dify), max_failed_pixels 0 (absolute count), output `tests/snapshots`. Configurable in workspace-root `kittest.toml`
  (`output_path`, `threshold`, `max_failed_pixels`, per-OS tables `[windows] [macos] [linux]`).
  Files: `{output_path}/{name}.png` (golden), `.new.png` (latest run), `.diff.png` (on mismatch), `.old.png` (backup when updating). Add `**/tests/snapshots/**/*.diff.png` and `*.new.png` to .gitignore.
  Update: `UPDATE_SNAPSHOTS=1` (or `true`) rewrites only failing snapshots and makes tests pass; `UPDATE_SNAPSHOTS=force` rewrites all (unsupported values panic).
  Multiple snapshots per test: `SnapshotResults` (`add`, `extend_harness`, `unwrap`, `into_result`) or `harness.take_snapshot_results()`; dropping unchecked results reports failures.
  Multi-OS differences: README says GPU/driver/backend differences cause pixel diffs; mitigations: software rasterizer (default adapter preference), `with_os(Nix)` default so shortcut text is OS-stable, per-OS thresholds, avoid MSAA.
  egui itself uses git LFS for snapshot PNGs. README recommends preferring non-image tests where possible.
- **Send**: `Harness` is NOT `Send` (stores `Box<dyn FnMut(&mut Ui) + 'a>` app closure and `Box<dyn TestRenderer>` without Send bounds; inferred from source, not compile-tested). Keep on the creating thread (same thread as cucumber runner is fine).
- **Multi-viewport / multiple windows**: Harness drives only the ROOT viewport (`ViewportId::ROOT`; `root_viewport_output()` used for repaint decisions). `ViewportCommand::InnerSize` and `Screenshot` from root/any viewport are handled; embedded viewports
  (egui's default when no native backend) render as in-root Areas so they appear in the one AccessKit tree. Separate deferred/immediate OS windows are not simulated. (Inference from source; UNVERIFIED by running.)
- **eframe apps**: feature `eframe` -> `Harness::new_eframe(|cc| MyApp::new(cc))` builds a fake `CreationContext`/`Frame` (`CreationContext::_new_kittest`) - no real window. Access app via `harness.state()`.
- **Android**: harness is host/desktop-only in practice: software wgpu adapter, `open`/`tempfile` deps are non-wasm; nothing in 0.36.2 runs tests on an Android device/emulator. UNVERIFIED whether crate compiles for `target_os=android` (likely yes since only the app code is exercised).
  Test the egui app logic on desktop via harness; test real Android separately.

### egui accessibility
- egui depends on `accesskit 0.24.1` unconditionally (a dependency, not feature-gated); eframe feature `accesskit` is in eframe's DEFAULT features (also default: `wgpu`, `wayland`, `x11`, `default_fonts`, `links`, `web_screen_reader`). New eframe feature `inspection` (egui_inspection, implies accesskit).
  README: "egui includes optional support for AccessKit, which currently implements the native accessibility APIs on Windows and macOS... enabled by default in eframe"; experimental built-in screen reader for web.
  Tree is only generated when `Context::enable_accesskit()` was called (eframe/winit do so when an AT connects; harness always does). API: `ctx.enable_accesskit()`, `ctx.disable_accesskit()`,
  `ctx.accesskit_node_builder(id, |node| ..) -> Option<R>` (customize nodes), `egui::accesskit_root_id()`, `Id::accesskit_id()`, `UiBuilder::accessibility_parent`.
  **`ctx.accesskit_placeholder_id` does not exist in 0.36.2** (grep found nothing).
- `Response::widget_info(|| WidgetInfo::...)` emits `OutputEvent` for screen readers (call after interaction); widget-to-Role map (response.rs): Label->Role::Label, Link->Link, TextEdit->TextInput, Button->Button, Image->Image,
  Checkbox->CheckBox, RadioButton->RadioButton, RadioGroup->RadioGroup, ComboBox->ComboBox, Slider->Slider, DragValue->SpinButton, ColorButton->ColorWell, Panel->Pane, ProgressIndicator->ProgressIndicator, Window->Window,
  ResizeHandle->Splitter, ScrollBar->ScrollBar, Other->Unknown. So `ui.label("x")` => Role::Label with text in `value`; custom-painted widgets need `widget_info`/`accesskit_node_builder` or they are invisible to kittest.
- **Android status**: eframe 0.36.2 README: runs native on "Linux, Mac, Windows, and Android"; features `android-native-activity` / `android-game-activity` (pass `android_app` in `NativeOptions`; `run.rs` errors if missing).
  **`accesskit` + `android-native-activity` is a compile_error** (accesskit_winit limitation) - AccessKit on Android requires `android-game-activity`. No iOS mention in egui README. Suspend/resume handling exists in the wgpu/glow integrations.
  egui README caveat: "interfaces are still in flux", every release has breaking changes.

---

## C) rquickjs - can a World holding a QML (JS) engine be Send?

- rquickjs **0.14.0 (2026-09-18)**; 0.13.0 2026-09-08; 0.12.2 2026-07-27; 0.12.1 2026-07-06; 0.12.0 2026-05-27; 0.11.0 2025-12-24. 5.09M downloads. Repo DelSkayn/rquickjs.
- Default (no `parallel`): `Runtime { inner: Rc<RefCell<RawRuntime>> }` and `Context` => **`!Send`/`!Sync`** (uses `Rc`/`RefCell`; `safe_ref.rs`).
- Feature `parallel` (`rquickjs/parallel` -> `rquickjs-core/parallel`, pulls `std`, `tokio/rt-multi-thread`): swaps `Rc<RefCell>` for `Arc<Mutex>`; `unsafe impl Send for RawRuntime`, `unsafe impl Send + Sync for Context`;
  `Runtime` becomes `Send` (Arc<Mutex<RawRuntime>> + mpsc sender); closures/promise hooks/interrupt handlers/native fns gain `Send` bounds (`ParallelSend` marker). README of crate: "Some experimental features like `parallel` may not works as expected. Use it for your own risk."
  Past soundness bug #151 ("Race condition for Context with parallel enabled", closed 2023). `Persistent` Send still open (#323).
- `AsyncRuntime` / `AsyncContext` (feature `futures`): `unsafe impl Send/Sync for AsyncRuntime` (inner is `Arc<Mutex<..>>`) in 0.14.0 source regardless of `parallel`; PR #680 "make async runtime and context Send/Sync without parallel feature" (closed 2026-04-27, merge status UNVERIFIED but code in 0.14.0 shows the impls).
- Conclusion: a World holding a sync `Runtime`/`Context` is `!Send` without `parallel`; with `parallel` it is Send but with `Send` bounds on every registered host fn and mutex overhead. Since cucumber does NOT require `Send` Worlds, just leave `parallel` OFF and keep everything on one thread.
- Also note: JS `Value<'js>`/`Ctx<'js>` are lifetime-bound to a `with` closure either way, so store `Context`/`Runtime`, not values, in the World.
