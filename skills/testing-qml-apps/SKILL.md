---
name: testing-qml-apps
description: "Specifies what a QML application does as Gherkin scenarios that run on Ljós and on Qt, and runs them on both. Use when the user asks to test a QML app or component, to write behaviour tests or feature files for their QML, or to check that it behaves the same on Ljós and on Qt."
---

# Testing QML apps

Specify what the user's QML files do as scenarios that run on Ljós and on Qt, and run them.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.
- `ljos:features_run` needs the server started with `--caps run` and `--features-runner <program>`, the program that runs scenarios.

## Steps

1. Read the files and list what a user sees and does: the texts, which controls are enabled, what a press or a key changes. `ljos:features_list` shows scenarios the project has already.
2. How a scenario is written here:
   - Use only the steps the resource `ljos://features/steps` lists, word for word: any other wording fails with "Step doesn't match any function". Pointer steps by position (`the user presses the pointer at X, Y`, then `releases`) run on Ljós and on Qt; finding a control by its role and name needs Ljós's accessibility bridge and fails on Qt.
   - A scenario sees only the files it declares. Give each with `Given the file "X.qml":` and a doc string labelled `qml`, at most 25 lines, and load it with `When "X.qml" is loaded`: after a Given, an And is a Given too.
   - Objects are found by `objectName`, not by `id`: give the objects you check one in the scenario's copy of the file.
   - The file starts with a `# Sources:` comment block, one source per line (the files it describes, Qt documentation URLs). A file tagged `@qt-parity` then has the line `# Verified on Qt 6.x.y (qml6, offscreen).` with the Qt version it passed on.
3. Tag the feature `@qt-parity`, one behaviour to a scenario.
4. `ljos:features_run` with `paths` set to the file and `backend` set to `both`, then `ljos:run_status`, waiting until the run has ended. A step that fails on Qt alone is Ljós-only wording: find one that runs on both. A scenario that is wrong about the file is fixed; one where Ljós and Qt differ is reported, never changed to pass.

The work is done when every scenario passes on both backends. A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
