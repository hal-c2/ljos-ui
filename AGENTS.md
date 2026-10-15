# Working on Ljós

Ljós is a general-purpose Rust UI toolkit that runs QML (Qt Quick + Qt Quick Controls compatible) on egui. Read `docs/ARCHITECTURE.md` before changing anything; it wins over the research reports in `docs/research/`.

## Rules

1. **Clean room.** Follow `CLEAN_ROOM.md`. Never read or copy Qt or Slint source.
2. **General purpose.** No crate, type, API, tool or milestone is written for one application. App-specific glue lives in that app's repository (ARCHITECTURE.md principle 9).
3. **Behaviour first.** New behaviour starts as a scenario in `features/` (see `features/README.md`), then code.
4. **egui stays in three crates:** `ljos_egui`, `ljos_shell`, and the B0 backend of `ljos_render`.
5. **No parser in shipped apps.** Runtime crates depend on `ljos_ir`, never on `qml_syntax` or `qml_front`.
6. **0 % CPU when idle.** Nothing polls. Work is scheduled by dirty state, timers, animations, input or messages.
7. **Bounded messages.** Threads talk to the Engine through `UiHandle` only.

## Commands

| Task | Command |
|---|---|
| Format, clippy | `mise run check` |
| Rust tests | `mise run test` |
| Gherkin ledger | `mise run features` |
| QML tests (`tst_*.qml`) | `mise run qttest` |
| External suite | `mise run features:acceptance <suite>` |
| qtdoc gallery | `mise run gallery` (after `mise run gallery:reference`) |
| Run one corpus example | `mise run gallery:run <id>` (no id lists them) |
| MCP authoring server | `mise run mcp` (also configured in `.mcp.json`) |
| Run an app off-screen (headless sway, own bus, cua-driver) | `mise run desktop:cua <file.qml> [args]`, `desktop:cua:exec <command…>`, `desktop:cua:call <tool> [json]`, `desktop:cua:stop` |

## Conventions

- Rust 1.98.1, edition 2024, resolver 3. Versions are pinned in `[workspace.dependencies]`.
- `mise-tasks/` are bash scripts with `#MISE` headers and `set -euo pipefail`.
- Use `thiserror` in libraries and `anyhow` only in binaries.
- Comments explain why, not what.
