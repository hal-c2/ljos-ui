---
name: localising-qml
description: "Makes every text a user reads in Ljós QML translatable with qsTr, and brings the translation catalogues up to date. Use when the user asks to translate, localise or internationalise QML, or to fix i18n lint findings."
---

# Localising QML

Make every text a user reads in the QML files or directories the user names translatable.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.

## Steps

1. `ljos:qml_lint` on the files. `i18n-untranslated-string` is a text written out without `qsTr`; `i18n-qstr-non-literal` is a `qsTr` whose argument no tool can extract.
2. Wrap each text in `qsTr`, with what varies put in by `.arg()`, not `+`.
3. `ljos:qml_format` and `ljos:qml_check` on the files you changed.
4. `ljos i18n extract` in a terminal brings the `.ts` catalogues up to date. If you cannot run it, say that it is left to do.
5. Then check, and repeat until none of these reports an Error:
   - `ljos check <dir>` in a terminal: the compiler and the static lints, for every file in the directory.
   - `ljos:qml_lint` on every file you touched.
   - `ljos audit <file.qml>` in a terminal, on the file that opens the window (on an application started with `ljos run --mcp`, the inspector's tool app_audit with `matrix` set to `full`). It draws the scene in light and dark, at high contrast, at 200 % text and in the small window, and finds what the lint cannot see: a target too small to press, a control off the window or covered, text cut off, contrast over what is behind it.

   Fix every Error. Fix every Warning, or keep it with `// ljos-allow(<code>): <reason>` where the finding is. The work is not done while any of the three reports an Error.

A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
