---
name: adding-accessibility
description: "Gives every control in Ljós QML a name and a role a screen reader can say, and holds that fixed with a scenario. Use when the user asks to make QML accessible or usable with a screen reader, or to fix a11y lint findings."
---

# Adding accessibility

Make the QML files or directories the user names usable by someone who cannot see them.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.
- `ljos:features_run` needs the server started with `--caps run` and `--features-runner <program>`, the program that runs scenarios.

## Steps

1. `ljos:qml_lint` on the files. Findings whose code starts with `a11y-` are items a screen reader cannot name; `ljos:design_rules` with the area `a11y` gives each rule's fix, and references/accessibility.md its reasons.
2. Set `Accessible.name` as a person would say it, in `qsTr`, and `Accessible.role`; `Accessible.ignored` for decoration. `ljos:type_lookup` on `QtQuick.Accessible` lists the rest.
3. `ljos:qml_check` on the files you changed.
4. Hold it fixed: `ljos:features_list` with the tag expression `@a11y` shows how accessibility is specified here; write a scenario that asks the accessibility tree for each name, run it with `ljos:features_run` and read `ljos:run_status`.
5. Then check, and repeat until none of these reports an Error:
   - `ljos check <dir>` in a terminal: the compiler and the static lints, for every file in the directory.
   - `ljos:qml_lint` on every file you touched.
   - `ljos audit <file.qml>` in a terminal, on the file that opens the window (on an application started with `ljos run --mcp`, the inspector's tool app_audit with `matrix` set to `full`). It draws the scene in light and dark, at high contrast, at 200 % text and in the small window, and finds what the lint cannot see: a target too small to press, a control off the window or covered, text cut off, contrast over what is behind it.

   Fix every Error. Fix every Warning, or keep it with `// ljos-allow(<code>): <reason>` where the finding is. The work is not done while any of the three reports an Error. A quiet lint is not proof: the audit sees what a screen reader is told when the application runs.

A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
