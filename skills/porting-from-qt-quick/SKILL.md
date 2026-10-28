---
name: porting-from-qt-quick
description: "Ports QML written for Qt Quick so that it runs on Ljós, keeping what it draws and does, and says what could not be ported. Use when the user has Qt QML to port, migrate or run on Ljós, or when Ljós rejects QML that works on Qt."
---

# Porting from Qt Quick

Port the Qt Quick QML the user points to, files in the project or QML they paste, to Ljós.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.

## Steps

1. `ljos:qml_check` and `ljos:qml_lint` on the source first: each Error is a place to change.
2. For a type or member the source uses, `ljos:api_status` (`QtQuick.Item.width`) says what Ljós has. Keep what is `supported`, stay inside the reason of a `partial` row, and for `dropped` or `unsupported` use what the reason names. Where there is no row, `ljos:type_lookup` says whether the type and member exist.
3. Change what a finding asks for and nothing else: what the files draw and do stays as it was.
4. `ljos:qml_format` on the result, and `ljos:binding_tier_explain` on each file: take its rewrites.
5. Then check, and repeat until none of these reports an Error:
   - `ljos check <dir>` in a terminal: the compiler and the static lints, for every file in the directory.
   - `ljos:qml_lint` on every file you touched.
   - `ljos audit <file.qml>` in a terminal, on the file that opens the window (on an application started with `ljos run --mcp`, the inspector's tool app_audit with `matrix` set to `full`). It draws the scene in light and dark, at high contrast, at 200 % text and in the small window, and finds what the lint cannot see: a target too small to press, a control off the window or covered, text cut off, contrast over what is behind it.

   Fix every Error. Fix every Warning, or keep it with `// ljos-allow(<code>): <reason>` where the finding is. The work is not done while any of the three reports an Error.

Say at the end what could not be ported, and why. A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
