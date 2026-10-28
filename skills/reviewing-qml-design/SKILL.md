---
name: reviewing-qml-design
description: "Reviews Ljós QML for UX, visual accessibility and motion problems, such as low contrast, targets too small to press, text cut off or controls off the window, and fixes them. Use when the user asks for a design review or a UI audit of QML, or to fix what `ljos audit` or the design lints report."
---

# Reviewing QML design

Review the QML file, directory or text the user gives for UX, visual accessibility and motion problems, and fix them.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.
- `ljos:features_run` needs the server started with `--caps run` and `--features-runner <program>`, the program that runs scenarios.

## Steps

1. `ljos:design_rules` lists every rule with its severity, how it is checked and its fix. An Error is certainly wrong for some user, a Warning very likely, a Hint is advice. For colours `ljos:design_palette` and `ljos:design_contrast`, for motion `ljos:design_motion`.
2. `ljos:qml_check` and `ljos:qml_lint` on the file or every file in the directory.
3. Read the fix of each code you got before you change anything. Take Hints that cost nothing.
4. `ljos:qml_format` on what you changed.
5. Then check, and repeat until none of these reports an Error:
   - `ljos check <dir>` in a terminal: the compiler and the static lints, for every file in the directory.
   - `ljos:qml_lint` on every file you touched.
   - `ljos audit <file.qml>` in a terminal, on the file that opens the window (on an application started with `ljos run --mcp`, the inspector's tool app_audit with `matrix` set to `full`). It draws the scene in light and dark, at high contrast, at 200 % text and in the small window, and finds what the lint cannot see: a target too small to press, a control off the window or covered, text cut off, contrast over what is behind it.

   Fix every Error. Fix every Warning, or keep it with `// ljos-allow(<code>): <reason>` where the finding is. The work is not done while any of the three reports an Error.
6. A scenario holds a fix in place: `ljos:features_list` shows how the design audit is specified here, and `ljos:features_run` with `ljos:run_status` runs yours.

The brief is references/agent-brief.md. Follow every line.

A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
