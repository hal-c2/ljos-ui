---
name: writing-scenarios
description: "Describes one behaviour of Ljós or of a QML type as a Gherkin scenario the Ljós runner can run, before or beside the code that has it. Use when the user asks for a scenario or a feature file for how a type or property behaves."
---

# Writing scenarios

Describe the behaviour the user states as a scenario the runner can run.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.
- `ljos:features_run` needs the server started with `--caps run` and `--features-runner <program>`, the program that runs scenarios.

## Steps

1. `ljos:features_list` with a path or a name near the behaviour: it may be specified already, and if not, it belongs in the file whose scenarios it lists.
2. `ljos:api_status` for the type or member the behaviour is about. Tag the scenario `@api-Module.Type.member` to join the scenarios its row names; what Qt does too is `@qt-parity`, what only Ljós does `@ljos-only`.
3. How a scenario is written here:
   - Use only the steps the resource `ljos://features/steps` lists, word for word: any other wording fails with "Step doesn't match any function". Pointer steps by position (`the user presses the pointer at X, Y`, then `releases`) run on Ljós and on Qt; finding a control by its role and name needs Ljós's accessibility bridge and fails on Qt.
   - A scenario sees only the files it declares. Give each with `Given the file "X.qml":` and a doc string labelled `qml`, at most 25 lines (cut a longer file down to what the scenario needs), and load it with `When "X.qml" is loaded`: after a Given, an And is a Given too.
   - Objects are found by `objectName`, not by `id`: give the objects you check one in the scenario's copy of the file.
   - The file starts with a `# Sources:` comment block, one source per line (the files it describes, Qt documentation URLs). A file tagged `@qt-parity` then has the line `# Verified on Qt 6.x.y (qml6, offscreen).` with the Qt version it passed on, which `qml6 --version` prints.
4. One behaviour to a scenario; say what is seen, not how it is built.
5. Run it with `ljos:features_run`, giving the path of the file, and read `ljos:run_status`. A scenario for behaviour not built yet fails: tag it `@backlog` and say so. Never change what a scenario says to make it pass.

A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
