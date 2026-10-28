---
name: writing-qml-components
description: "Writes a new QML component or screen for a Ljós application, and checks it until the compiler, the lint and the design audit report no Error. Use when the user asks to add, build or design a QML component, view, panel, dialog or screen for Ljós."
---

# Writing QML components

Write the Ljós QML component the user asks for, for the purpose they give.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.
- `ljos:features_run` needs the server started with `--caps run` and `--features-runner <program>`, the program that runs scenarios.

## Steps

1. `ljos:api_status` for every Qt Quick type and member you plan to use (`QtQuick.Item.width`). A row that is `unsupported` or `dropped` says what to use instead.
2. The behaviour before the code: `ljos:features_list` shows the scenarios nearest to it; write a `.feature` that says what the component does. How a scenario is written here:
   - Use only the steps the resource `ljos://features/steps` lists, word for word: any other wording fails with "Step doesn't match any function". Pointer steps by position (`the user presses the pointer at X, Y`, then `releases`) run on Ljós and on Qt; finding a control by its role and name needs Ljós's accessibility bridge and fails on Qt.
   - A scenario sees only the files it declares. Give each with `Given the file "X.qml":` and a doc string labelled `qml`, at most 25 lines, and load it with `When "X.qml" is loaded`: after a Given, an And is a Given too.
   - Objects are found by `objectName`, not by `id`: give the objects you check one in the scenario's copy of the file.
   - The file starts with a `# Sources:` comment block, one source per line (the files it describes, Qt documentation URLs). A file tagged `@qt-parity` then has the line `# Verified on Qt 6.x.y (qml6, offscreen).` with the Qt version it passed on.
3. `ljos:design_rules` lists the design rules; references/agent-brief.md has them as one line each. Follow every line.
4. Write the QML, then `ljos:qml_format`, `ljos:qml_check` and `ljos:qml_lint` on it.
5. `ljos:binding_tier_explain` on each file: take the rewrites it offers.
6. Run the scenarios with `ljos:features_run`, giving the path of the `.feature`, and read `ljos:run_status`.
7. Then check, and repeat until none of these reports an Error:
   - `ljos check <dir>` in a terminal: the compiler and the static lints, for every file in the directory.
   - `ljos:qml_lint` on every file you touched.
   - `ljos audit <file.qml>` in a terminal, on the file that opens the window (on an application started with `ljos run --mcp`, the inspector's tool app_audit with `matrix` set to `full`). It draws the scene in light and dark, at high contrast, at 200 % text and in the small window, and finds what the lint cannot see: a target too small to press, a control off the window or covered, text cut off, contrast over what is behind it.

   Fix every Error. Fix every Warning, or keep it with `// ljos-allow(<code>): <reason>` where the finding is. The work is not done while any of the three reports an Error.

A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
