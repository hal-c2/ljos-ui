# Skill evals: results

Four tasks, three models (haiku, sonnet, opus), each run once in a fresh
project with the `ljos` MCP server (`--caps fs,run` and a features runner),
`claude -p` with only the project's settings, the same tool allowlist with and
without skills, and a spend cap. Each grader is a command that passes or
fails (see the task files). `baseline` has the MCP server and no skills;
`skill` has every skill in `skills/` installed in the project.

## Baseline (no skills)

| Task | Grader | haiku | sonnet | opus |
|---|---|---|---|---|
| write-component | check | pass | pass | pass |
| | lint | pass | pass | pass |
| | audit | pass | **FAIL** | **FAIL** |
| | content | pass | pass | pass |
| port-from-qtquick | check | pass | pass | pass |
| | lint | pass | pass | pass |
| | behaviour | pass | pass | pass |
| test-an-app | ljos | pass | pass | **FAIL** |
| | qt | pass | pass | **FAIL** |
| | features-lint | **FAIL** | **FAIL** | **FAIL** |
| embed-in-rust | build | pass | **FAIL** | pass |
| | test | pass | **FAIL** | **FAIL** |
| | content | pass | pass | pass |

29 of 39 grades pass; 5 of 12 runs pass every grader. Spend: 3.72 USD.

A first test-an-app baseline ran on a server whose `features_run` ran
nothing for one file and the runner's own scenarios for a run with no paths:
haiku and sonnet failed all three graders, opus failed features-lint. That
is a server bug no skill can fix, so it was fixed (a run always passes
`--input`, one file as a group of one) and the baseline above was run again
on the fixed server, so that no skill is credited with it.

## Final (with skills)

| Task | Grader | haiku | sonnet | opus |
|---|---|---|---|---|
| write-component | check | pass | pass | pass |
| | lint | pass | pass | pass |
| | audit | pass | pass | pass |
| | content | pass | pass | pass |
| port-from-qtquick | check | pass | pass | pass |
| | lint | pass | pass | pass |
| | behaviour | pass | pass | pass |
| test-an-app | ljos | pass | pass | pass |
| | qt | pass | pass | pass |
| | features-lint | pass | **FAIL** | **FAIL** |
| embed-in-rust | build | pass | pass | pass |
| | test | pass | pass | pass |
| | content | pass | pass | pass |

37 of 39 grades pass; 10 of 12 runs pass every grader. Spend: 4.46 USD.

The rows are the runs of the skill text as committed: write-component,
port-from-qtquick and test-an-app from the first iteration, embed-in-rust
from the second.

## What failed, and what fixed it

| Failure in the baseline | Runs | Fixed by |
|---|---|---|
| No run used `ljos audit`; the Basic `Slider` is 13 px high (a11y-target-size) and a `ComboBox` was off the small window at 200 % text (layout-offscreen) | write-component sonnet, opus | The loop every QML skill ends with: `ljos check`, `ljos:qml_lint`, `ljos audit` on the file that opens the window, repeated until none reports an Error, and what the audit sees that the lint cannot. All three ran the audit and passed. |
| Guessed step wording, then no step catalogue, then gave up | test-an-app opus | The scenario rules in testing-qml-apps: steps from `ljos://features/steps` word for word, pointer steps by position because a role and a name need Ljós's accessibility bridge, files given inline and loaded, objects by `objectName`. Every run passed on Ljós and on Qt. |
| No `# Sources:` block, no `# Verified on Qt` line | test-an-app all | The same rules name both headers. haiku now passes. sonnet and opus wrote the `# Sources:` block but not a valid version line, because the allowlist does not let them run `qml6` and nothing else they can reach says the version (see below). sonnet also wrote a 29-line doc string. |
| Chose the shipping build (`build.rs`, `include_qml!`) and could not write the type dump, because `LJOS_TYPES=write cargo test` is not a `cargo` command to the allowlist | embed-in-rust sonnet, opus | embedding-qml-in-rust step 1: `dev-qml` and `App::qml_dir` while the program is written, with no build script and no dump; the shipping build and when its dump is written; ask the user if it cannot be run. Every run built and tested. |
| Read the crate's source to learn the API | embed-in-rust all | The skill's four references (native types, models, threads, testing). Every run opened them, and every run still read some of the crate's source (3 to 7 files) for signatures. |

## The iterations

1. **Iteration 1** ran all twelve. One opus embed run failed `content`
   because it called `setup.handle()`, which is the real accessor. The skill,
   copied from `docs/site/rust/threads-and-uihandle.md`, named
   `setup.ui_handle()`, which does not exist. Both the doc example and the
   skill were fixed, and the grader was changed to accept `.handle()`. Under
   that grader, the iteration-1 run passes.
2. **Iteration 2** ran embed-in-rust and test-an-app again.
   - **Embed-in-rust.** The rerun on the corrected skill passed all graders
     for all three models.
   - **Test-an-app.** Two sentences were added: `qml6 --version` prints the
     Qt version, and a longer file is cut down to what the scenario needs.
     Every model tried `qml6`, was refused by the allowlist, and then left
     the version line out. All three failed features-lint, and the doc
     strings stayed as long as before. Both sentences failed the no-op test,
     so they were taken out again, and the final table uses iteration 1 for
     test-an-app.

## Cost

| Task | Baseline (haiku / sonnet / opus, USD) | Final |
|---|---|---|
| write-component | 0.01 / 0.08 / 0.17 | 0.03 / 0.39 / 0.82 |
| port-from-qtquick | 0.01 / 0.07 / 0.20 | 0.01 / 0.15 / 0.41 |
| test-an-app | 0.03 / 0.33 / 0.22 | 0.02 / 0.20 / 0.35 |
| embed-in-rust | 0.23 / 0.66 / 1.71 | 0.34 / 0.48 / 1.26 |

writing-qml-components costs more because it does what its prompt always
said: write a scenario before the QML and run it, then audit.

## Left to do

- A run on Qt should say which Qt it ran. Until it does, an agent that may
  not run `qml6` cannot write the `# Verified on Qt` line from anything it
  has.
- No tool an agent has in a user's project checks a feature file against
  the rules `features-lint` holds it to: 25-line doc strings, and the
  headers. `features_run` or `ljos` could.
- `ljos audit` is a command, not a tool of either MCP server, except as
  `app_audit` on a running application.
- In the audit, a `ComboBox` inside a `ScrollView` was reported as 100 %
  covered by the scroll bar (layout-occluded); this looks like a false
  positive. The Basic `Slider`'s default height of 13 px fails
  a11y-target-size.
- port-from-qtquick passed without skills. Its skill is kept because the
  prompt exists, not because the eval shows a gain.
