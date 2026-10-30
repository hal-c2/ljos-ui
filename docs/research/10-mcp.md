# 10 — MCP built into Ljós

Status: research report, 2026-10-10. Audience: Ljós architects. Fits ARCHITECTURE.md §3 (threads/messages), §4 (workspace), §9 (lanes/JsHost), §12 (hot reload), §14 (a11y), §17 (HAL-C2), §18 (testing).
Clean-room note: only public specs, docs and permissively licensed Rust crates were read. No Qt or Slint source.

---

## 1. Summary of key findings

- **Spec.** The current MCP revision is **2026-07-28** (final, published 2026-07-28; RC was 2026-05-21). It replaces 2025-11-25. It is a *stateless* protocol:
  - no `initialize` handshake and no `Mcp-Session-Id`;
  - each request carries `_meta."io.modelcontextprotocol/protocolVersion"` and `clientCapabilities`;
  - a mandatory `server/discover` RPC;
  - `subscriptions/listen` replaces GET streams and `resources/subscribe`;
  - server→client requests (sampling, elicitation, roots) become **Multi Round-Trip Requests** (`resultType: "input_required"`);
  - SSE resumability is gone.
- **Deprecated in 2026-07-28:** Roots, Sampling and Logging (twelve-month minimum window), HTTP+SSE transport, and Dynamic Client Registration (replaced by Client ID Metadata Documents). Ljós should **not** build on any of them.
- **Tasks** are no longer core. They are the official extension `io.modelcontextprotocol/tasks` (poll with `tasks/get`, `tasks/update`; no `tasks/list`). Usable for long-running test runs, but optional.
- **Stable and useful for us:** tools (with `outputSchema`, `structuredContent` of any JSON value, `resource_link` results, annotations), resources and templates, prompts, completion, pagination, `ttlMs`/`cacheScope` caching hints, deterministic `tools/list` order (helps LLM prompt-cache hits).
- **Transports.** stdio (newline-delimited JSON-RPC) and Streamable HTTP (single POST endpoint). For HTTP the spec says servers MUST validate `Origin` (403 on mismatch), SHOULD bind 127.0.0.1, and SHOULD authenticate. The spec explicitly allows **custom transports over Unix sockets/TCP reusing stdio framing**. That fits the Android `adb forward` story.
- **Rust SDK.** `rmcp` (official, modelcontextprotocol/rust-sdk) is at **3.5.1 (2026-10-05)**, MSRV **1.88** (Ljós pins 1.98.1). It implements 2026-07-28 and stays compatible with 2025-11-25 and earlier.
  - Macros: `#[tool_router(server_handler)]`, `#[tool]`, `#[prompt_router]`.
  - `schemars ^1.0` (2020-12 schemas), tokio-native.
  - `transport-io`, `transport-streamable-http-server`, `auth` (oauth2), `elicitation`, `request-state`.
  - 3.x had a breaking-change wave (2.x→3.x migration notes: `CallToolResponse` wrappers, `#[non_exhaustive]` unions, `Meta` split, tasks API replaced). Expect churn.
  - Alternative `rust-mcp-sdk` 2.x also targets 2026-07-28 (claims 110/110 server conformance) but is the minority choice.
- **Prior art converges on one shape:** an accessibility-tree **text snapshot with short stable refs** (Playwright MCP `browser_snapshot`; HAL-C2's `agent-device snapshot -i` `@eN` refs), role+name locators, optional screenshots, and **opt-in capability groups** (`--caps=vision,devtools`).
  - Flutter's `dart_mcp_server` combines authoring tools (`analyze_files`, `dart_format`, `run_tests`, `pub`) with runtime tools (`get_widget_tree`, `take_screenshot`, `hot_reload`, `get_runtime_errors`) in one stdio server.
  - egui-specific: **`egui_inspection`** (AccessKit tree, input injection, screenshot over TCP `127.0.0.1:5719`, no auth) with an `egui_mcp` server, and **`eguidev`** (in-process, string-tagged widgets, `script_eval` Luau, `script_api`). Both validate our design; neither has hot reload, binding provenance or a deterministic clock.
- **HAL-C2 constraint.** HAL-C2's preview toolkit is browser-shaped: `locator` (Playwright selectors such as `role=button[name='Send']`), `selector` (CSS), or x/y; snapshots return `interactiveElements{tag,role,name,selector,x,y,width,height}`, `accessibilityTree`, console/network/action timeline, and a PNG. Results are capped at ~60 KB text because **Claude Code drops MCP results above 25k tokens**. Device tools are a deliberately small surface and delegate driving to the `agent-device` CLI with `@eN` refs.
- **Recommendation in one line.** Two servers sharing one crate: `ljos mcp` (authoring, stdio, no app) and an in-app **inspector** (dev builds only, loopback + token, read-only by default) whose tool names and snapshot format mirror HAL-C2's `preview_*` with a *role/name locator* dialect. HAL-C2 should **connect directly** to the app's inspector and expose it through its own `preview_*` tools (a thin adapter), rather than proxying a second MCP server through its tool list.

---

## 2. Detailed findings

### 2.1 MCP spec state (Oct 2026)

Source: changelog for 2026-07-28 (URL in §5).

| Area | 2026-07-28 status | Ljós stance |
|---|---|---|
| Lifecycle | Stateless. No `initialize`/`initialized`. `server/discover` MUST be implemented (versions, capabilities, identity). Version mismatch → `UnsupportedProtocolVersionError` (-32022). | rmcp handles it. We implement `discover` via rmcp and test with the conformance runner. |
| Sessions | `Mcp-Session-Id` removed. Cross-call state uses explicit server-minted **handles** passed as tool args. | Our *refs* and `snapshot_id` are exactly such handles. Matches the spec's "Stateful tools" guidance. |
| Transports | stdio; Streamable HTTP (POST only, JSON or per-request SSE, closing the stream = cancel). Required headers `MCP-Protocol-Version`, `Mcp-Method`, `Mcp-Name`. `x-mcp-header` optional. Custom transports over Unix sockets/TCP should reuse stdio framing. HTTP+SSE Deprecated. | stdio for authoring and local agents. Streamable HTTP on loopback for the inspector. Optional Unix socket with stdio framing for mobile/CI. |
| Tools | `inputSchema`/`outputSchema` = any JSON Schema 2020-12. `structuredContent` any JSON. `resource_link`. Names `[A-Za-z0-9_.-]`, ≤128 chars. Tool errors: `isError: true` for model-fixable problems, JSON-RPC errors for malformed calls. Annotations (read-only, destructive, idempotent, open-world) are *untrusted hints*. | Every tool gets `outputSchema` + a text mirror. Annotations set honestly, but real gating is capability scopes in our server. |
| Resources | Templates, `resources/read`, not-found is -32602. Subscriptions via `subscriptions/listen` (`resourceSubscriptions`). `ttlMs`/`cacheScope` required on list/read results. | `ljos://` resources with long `ttlMs` for static docs/types. Short or zero for live state. |
| Prompts, completion | Unchanged in spirit; `prompts/get` has `Mcp-Name`. Completion for resource-template and prompt args. | Prompts are cheap; completion for `{module}/{type}` is useful. |
| Elicitation / sampling / roots | Elicitation via MRTR (`input_required`). Sampling, Roots, Logging **Deprecated**. | Avoid sampling, roots, logging notifications. Project root comes from CLI arg/cwd. Elicitation is not needed v1. |
| Tasks | Extension `io.modelcontextprotocol/tasks`: server may return a task handle; poll `tasks/get`, `tasks/update`, `tasks/cancel`. | Use for `features_run`/`cargo`-class operations over ~10 s. Fallback: return a `run_id` handle + `run_status` tool, which works with every client today. |
| Auth (HTTP) | OAuth 2.1-style; DCR Deprecated in favour of Client ID Metadata Documents. `iss` validation (RFC 9207). | Not applicable to a loopback dev inspector. Use a bearer token (same shape HAL-C2's `McpHttpServer` already uses: `Authorization: Bearer`, `www-authenticate: Bearer`). OAuth only if a remote app-agent server ever ships. |
| Extensions | `extensions` in capabilities. | We may later publish `dev.ljos/*` extension metadata; not v1. |
| Telemetry | OpenTelemetry `traceparent` in `_meta`. | Propagate into `tracing` spans; cheap win. |

Practical consequence: with no protocol sessions, **statelessness is the default** and long-lived state must live in app-owned handles. This suits an inspector, whose "session" is the running app itself.

Client reality check: Claude Code and others still support 2025-11-25 servers. rmcp 3.x speaks both; do not hard-require the new revision in tests.

### 2.2 Rust SDKs

**`rmcp` 3.5.1** (docs.rs, 2026-10-05; crates.io lists 3.5.0 then 3.5.1):
- MSRV 1.88; Ljós's pin of 1.98.1 is fine.
- Default features: `server`, `macros`, `base64`. `server` pulls `schemars` (^1.0), `transport-async-rw` (tokio-util codec), `uuid`.
- Transports: `transport-io` (stdio), `transport-streamable-http-server` (needs `server-side-http`: http, http-body-util, tower, sse-stream, rand, uuid), `transport-worker` (in-process, **ideal for tests**), `transport-child-process` and `transport-streamable-http-client-reqwest` (client side; useful for the conformance/integration tests and for HAL-C2-style forwarding), `transport-streamable-http-client-unix-socket` (client).
- Other features: `auth` (oauth2, reqwest), `elicitation`, `request-state` (HMAC-sealed MRTR `requestState`), `client`.
- Axum is a *dev* dependency only; the HTTP server transport is a tower `Service` we can mount on hyper or axum. We will mount it on hyper directly to avoid axum, or accept axum as a dev-crate-only dep.
- Macros: `#[tool_router(server_handler)]` for tools-only servers; `#[tool]` per method; `#[prompt_router]`/`#[prompt]`; `Parameters<T>` where `T: Deserialize + JsonSchema`; structured output via returning a `Json<T>` type with `outputSchema` derived (verify exact wrapper name in 3.5; the README example returns `String`).
- Tasks extension implemented (`rmcp::task_manager::TaskManager`, `CreateTaskResult`).
- Legacy HTTP+SSE intentionally **not** provided.
- Maturity: official, rapid releases (3.0 breaking wave: `CallToolResponse`, `#[non_exhaustive]` enums, `Meta` split, tasks API replaced; migration in rust-sdk discussion #969). Pin an exact minor (`=3.5`) and budget a migration PR per quarter.

**Alternatives.** `rust-mcp-sdk` 2.0.0 (2026-07-28, stdio + Streamable HTTP + OAuth, claims 110/110 server and 440/440 client conformance; 1.x LTS on 2025-11-25). Others (`pmcp`, `prism_mcp_rs`, `mcpkit`, `mcp_protocol_sdk`) are single-maintainer. **Pick rmcp**: official, tracks the spec first, macros reduce boilerplate, and HAL-C2 may later want to speak to the same crate from Rust. Keep our own tool handlers behind a small `McpSurface` trait (§3.5) so swapping SDK costs ~1 file.

**Dependency weight and placement.** rmcp+schemars+tokio+http stack adds roughly 40–60 crates beyond what the tokio services already bring (estimate; measure with `cargo tree -e normal --prefix none | sort -u | wc -l`). Rules:
- Never in a shipped app by default. The inspector crate is behind a Cargo feature `devtools-mcp` in `ljos_dev`/`ljos_mcp`, and `ljos_shell` links it only when the app enables it.
- `ljos_cli` links the authoring server unconditionally (it is a dev tool; the parser dependency is allowed there per §4).
- Because MCP is JSON-RPC over tokio, it lives on the tokio threads already in §3.2, with no new runtime.

### 2.3 Prior art and what works for LLMs

| Project | Shape | Lessons |
|---|---|---|
| **Playwright MCP** | `browser_snapshot` returns an accessibility snapshot as text with element refs. Actions take `target` (ref or selector). Params `depth`, `boxes`, `filename`; `browser_find` is "cheaper than a whole snapshot". Capabilities `--caps=vision,pdf,devtools`; vision mode adds coordinate tools. "Not a security boundary"; `run_code_unsafe` is RCE-equivalent. README itself notes that MCP's big schemas and verbose trees cost tokens vs a CLI. | Default to semantic snapshot, not screenshots. Provide depth limit + find/query + write-to-file for large outputs. Opt-in capability groups. Name dangerous tools `*_unsafe`/gate them. |
| **Chrome DevTools MCP** | Debug/perf/network; `--slim` mode for basic tasks; browser starts lazily on first tool call; usage stats opt-out. | Provide a *slim* profile (few tools) because each tool schema costs tokens on every turn. Lazy start. |
| **Dart/Flutter `dart_mcp_server`** | stdio only. Static tools (`analyze_files`, `dart_format`, `dart_fix`, `run_tests`, `pub`, `signature_help`, `hover`, `resolve_workspace_symbol`) plus runtime tools (`get_widget_tree`, `get_selected_widget`, `take_screenshot`, `hot_reload`, `get_runtime_errors`) via the Dart Tooling Daemon; relies on client `roots`. | One binary serving both authoring and runtime is proven. We split the *server* (no app vs live) but keep one crate and one catalogue style. Roots are being deprecated, so pass the project path explicitly. |
| **egui_inspection / egui_mcp** | TCP request→response protocol (`GetTree` = AccessKit tree, `HandleEvents`, `Screenshot`, `Resize`), `127.0.0.1:5719`, enabled by `EGUI_INSPECTION` env + eframe feature, **no auth**; screenshots need a rendered frame (time out when occluded). | Using the AccessKit tree as the agent-facing tree works on egui. Our a11y tree gets us the same, but screenshots must come from the offscreen reference renderer, not a window, to avoid the occlusion failure. Add auth. |
| **eguidev** | In-process runtime attached via `eguidev_runtime::attach`; widgets tagged with string ids; `script_eval` (Luau) does many operations per call; `script_api` returns docs; `devtools` Cargo feature; inert when not attached. | Batch/scripted execution reduces round trips and tokens. For Ljós we get string ids for free (`objectName`, `Accessible.name`). A `batch` tool (array of steps, one response) gives the same benefit without a second language. |
| **HAL-C2 `agent-device`** | CLI with `snapshot -i` returning `@eN` refs, `click @e3`, `fill @e5 "text"`; HAL-C2 gives only list/open/screenshot/close tools. Rationale in code: wrapping the CLI "would only lag behind it". | Short ephemeral refs scoped to a snapshot are the norm. |
| **Godot/Unity/ui-toolkit MCP** | Scene/node inspection (tree, resolved styles, bounds), screenshots of whole screen or selected element, play-mode bridge. | Resolved style/property read and per-element screenshot are valued. Ljós's binding provenance is a differentiator. |
| **Tauri MCP plugins** | Searched; nothing authoritative found. | n/a |

**Tool-shape guidance distilled**
1. Snapshot = indented text outline (role, quoted name, key state, ref), not JSON, for the model; `structuredContent` carries the same data machine-readably. Outline lines of ~60 chars cost ≈ 15–20 tokens; a 500-node screen ≈ 8–10k tokens. Cap and paginate.
2. Refs are short (`e12`), valid only for a `snapshot_id`/epoch, and also resolvable by **role+name locator** so agents can act without a fresh snapshot.
3. Return a *delta* ("what changed") after actions rather than a full snapshot unless asked.
4. Screenshots are opt-in (`image: true`), downscaled (max 1280 px long edge), PNG; never default.
5. Hard size caps below Claude Code's 25k-token ceiling: HAL-C2 uses 60,000 bytes of text. We use **40 KB default / 80 KB max** per tool result plus `truncated:true`, `next_cursor`.
6. Keep the default tool list small (≈ 25 per server), deterministic order, concise descriptions (tool schemas are paid on every turn).

### 2.4 HAL-C2 integration facts (read-only inspection)

- `toolkits/preview/tools.ts`: `preview_status|open|navigate|resize|set_appearance|snapshot|click|type|press|scroll|evaluate|wait_for|recording_start|recording_stop`. Annotations used: Readonly/Destructive/Idempotent/OpenWorld. `preview_snapshot` is a separate toolkit so it can be offered separately (`PreviewSnapshotToolkit`).
- Inputs: `tabId` optional (session's current tab by default), exactly one of `locator` (Playwright selector, e.g. `role=button[name='Send']`, `text=Continue`), `selector` (legacy CSS), or `x`,`y`. `press` takes `key` plus `modifiers`. `evaluate` returns `{value}` wrapped in an object because MCP `structuredContent` must be an object for Claude Code (a comment in the code notes Claude Code rejects non-object results; even though the 2026-07-28 spec allows any JSON, **wrap in objects**). 64 KB cap on evaluated value.
- Snapshot contract (`PreviewAutomationSnapshot`): `url,title,loading,visibleText,interactiveElements[{tag,role,name,selector,x,y,width,height}],accessibilityTree,consoleEntries,networkEntries,actionTimeline,screenshot{mimeType,data,width,height}`. Server-side trimming constants: 60 KB text, 8000 chars visible text, 200 chars per element name, 40 log entries.
- Capability gating: `McpInvocationContext.requireMcpCapability("preview" | "device")`, per-thread bearer token resolved by `McpSessionRegistry`; unauthorized → 401 `www-authenticate: Bearer`.
- Device toolkit: `device_list|open|screenshot|close`; driving is delegated to `agent-device`.

### 2.5 Architecture fit (ARCHITECTURE.md)

- **Threading (§3.2–3.3).** The MCP server runs on the tokio workers (the "devtools socket" row already exists). Requests become `UiMsg::Devtools(DevReq)` via a bounded `UiHandle::send` (fail-fast `Full` → tool error `busy`, retryable). Replies return through a `oneshot`. The UI drains in frame-step-1 under the `SliceBudget`; a `DevReq` is a `Job` in the **Input lane for input synthesis** and **Deferred lane for snapshots/queries** (so a large snapshot yields via `JobPoll::Yield` and cannot blow a frame). `send_coalesced` is used for high-frequency streams (metrics subscriptions).
- **Idle.** No timers on the UI side. A request sets the wake flag (existing path) and the loop sleeps again afterwards. The tokio side is parked on sockets. 0 % idle CPU is preserved. `subscriptions/listen` streams are push-only from events that already exist (reload applied, log line), never polled.
- **Determinism.** `advance` (virtual clock) is a tool only when the app was started with `--test-clock`; otherwise `wait_for` uses wall time with the same predicate evaluation. The kittest path (`ljos_test`) can host the *same* inspector in-process via the `transport-worker`, so a BDD scenario and an agent session share semantics.
- **A11y reuse (§14).** Snapshot is generated from the AccessKit tree plus `ObjectKey`. The a11y pass is normally `update_if_active`; the inspector registers as an "active consumer" for the duration of a request (build tree on demand, then release), so there is no cost when no agent is attached.
- **Refs.** AccessKit `NodeId` for static items is `(arena index, generation)` and `(list node, model key)` for delegates. Two ref forms:
  - ephemeral `e<N>`, per snapshot epoch (what the model sees); mapped server-side to NodeId;
  - durable **path locator**: `role=button[name="Save"]`, `#objectName`, or `ObjectKey` path (`Main.qml:saveBtn`), valid across hot reload (§12.1 identity rules).
  Stale ref → tool error `stale_ref` with a one-line hint to re-snapshot, or auto-resolve by role+name if unique.
- **Hot reload (§12).** The authoring server's `qml_check` and the live inspector's `reload` share `ljos_compiler`. After `reload`, the inspector emits `notifications/resources/updated` for `ljos://app/tree` (via `subscriptions/listen`) so an agent can wait on `app_wait_for {reload_generation}`.
- **JS eval (§9.2).** `app_eval` runs through `JsHost::enter(slice, f)` with the interrupt deadline; gated and logged; output capped.

---

## 3. Recommendations for Ljós

### 3.1 Decisions

| # | Decision | Why | Rejected alternative |
|---|---|---|---|
| M1 | **rmcp 3.5** pinned to a minor, with tools-only `#[tool_router]` handlers behind our own `McpSurface` trait. | Official, tracks spec, tokio-native, `transport-worker` for tests. | `rust-mcp-sdk` 2.x (viable fallback); hand-rolled JSON-RPC (spec churn). |
| M2 | **Two servers, one crate family.** `ljos mcp` (authoring, stdio) and the in-app inspector (Streamable HTTP, loopback). | Authoring needs no app; inspector needs the live engine. Different trust levels. | One mega server (mixes read-only file tools with app mutation; huge tool list per turn). |
| M3 | Target **2026-07-28**, remain compatible with 2025-11-25 via rmcp. Do not use Sampling, Roots, Logging, HTTP+SSE, DCR. | Deprecated with ≥12-month window; avoids rework. | Using `roots` for project root (use `--project` arg). |
| M4 | Inspector tool names: **`app_*` generic**, plus a thin **`preview`-compatible adapter** in HAL-C2 (§3.6). | Our tools describe QML concepts; HAL-C2 keeps its browser-flavoured names. | Naming ours `preview_*` (implies browser; `selector` CSS/`evaluate` semantics don't map). |
| M5 | Snapshot = text outline + structured mirror; refs `eN` per epoch + durable role/name/objectName locators. | Matches Playwright/agent-device prior art; stable across hot reload. | Screenshot-first; raw AccessKit JSON dump (too many tokens). |
| M6 | Read-only by default; capabilities `inspect` (default), `input`, `mutate`, `eval`, `fs`, requested at launch (`--mcp-caps`). | Annotations are untrusted per spec; enforce server-side. | Single on/off. |
| M7 | Per-process **bearer token**, loopback or Unix socket only, `Origin` validation (403), token written to a `0600` file `target/ljos/mcp-<pid>.json`. | Spec MUSTs; egui_inspection's no-auth is a footgun. | No auth; OAuth. |
| M8 | App-defined tools (`Ljos.Mcp` module): **post-M4, out of v1**; design the registry now so the inspector tools and app tools share one `ToolRegistry`. | Valuable but a product/security feature; a11y tree already makes apps drivable. | Ship in M2 (scope creep). |
| M9 | Long operations: return `run_id` handle + `run_status`/`run_cancel` now; adopt the tasks extension when HAL-C2 and Claude Code clients support it. | Works with every client today. | Depend on tasks extension in v1. |
| M10 | Result caps: 40 KB default / 80 KB hard text per result; screenshots ≤1280 px; always `truncated` + `next_cursor`. | Claude Code drops >25k tokens (HAL-C2 uses 60 KB). | Unbounded. |

### 3.2 Crate graph

```
ljos_mcp_core   (no egui, no engine)  rmcp, schemars, serde; McpSurface, ToolRegistry, caps/token,
                                       size-capping, snapshot text renderer, ref table; TESTED standalone.
ljos_mcp_author (dep: ljos_mcp_core, ljos_compiler, qml_front, qt_types, ljos_bdd(opt))
                                       -> linked only by ljos_cli (`ljos mcp`)
ljos_dev  [feature = "mcp"]            inspector: dep ljos_mcp_core, ljos_runtime, ljos_ir, ljos_a11y;
                                       adds UiMsg::Devtools handler + tokio listener.
                                       Does NOT name egui; screenshots through ljos_render's R backend.
ljos_cli                               `ljos mcp`, `ljos run --mcp[=caps]`, `ljos mcp-client` (smoke / call a tool)
ljos_test / ljos_bdd                   host inspector in-process (transport-worker) for steps & MCP tests
```

`egui` stays confined to the three existing crates. Release builds: `ljos_dev` is not a dependency of the app crate unless the app enables its `devtools` feature; `ljos build --release` fails if `mcp` is enabled without `--allow-mcp-in-release`. Milestone placement in §3.7.

### 3.3 Authoring server — `ljos mcp` (stdio)

Started as `ljos mcp --project <dir> [--caps run]`. Default profile exposes the read-only/analysis tools; `run`-class tools (spawn cargo/qml6) need `--caps run`.

**Tools**

| Name | Semantics (one line) | Annotations |
|---|---|---|
| `qml_check` | Parse+resolve one file or glob; returns diagnostics `{file,line,col,severity,code,message,fix?}` (structured, ≤200 items, paginated). | readOnly |
| `qml_lint` | Lint set incl. a11y audit lints (§14: unnamed actionable item, image without name, `qsTr` non-literal, missing `id` on stateful siblings). | readOnly |
| `qml_format` | Format text or file; `write:false` returns the diff; `write:true` needs `fs` cap. | idempotent |
| `qml_apply_fix` | Apply a diagnostic's suggested fix by `code`+location (dry-run default). | needs `fs` |
| `type_lookup` | Type/property/signal/method/enum info from the type registry + qmltypes; includes base type chain, default property, attached types. | readOnly |
| `api_status` | Query `ledger/qtquick.toml`: `supported/partial/dropped/unsupported` for a `Module.Type[.member]`, with reason and the `@api-` feature that proves it. | readOnly |
| `docs_search` | Keyword search over Ljós docs + ledger notes (not Qt source; Qt doc URLs only as links); returns ≤8 snippets with `ljos://docs/...` links. | readOnly |
| `binding_tier_explain` | For an expression: tier 0/1/JS classification, why, and how to move it down a tier (§5.4). | readOnly |
| `qt_compare` | Run `qml_compare` against real Qt for a component/scenario; returns pixel and content scores; PNG as `resource_link`. Requires Qt installed. | needs `run` |
| `features_list` | List `.feature` scenarios by tag/path (`@backlog`, `@api-...`). | readOnly |
| `features_run` | Run BDD scenarios via `ljos_bdd` (globs, tag expr); returns `run_id`. | needs `run` |
| `kittest_run` | Run a named `cargo test`/kittest target. | needs `run` |
| `run_status` / `run_cancel` | Poll/cancel a run; returns summary + failing scenarios + paths of `LJOS_SHOTS` images. | readOnly / destructive |
| `component_scaffold` | Generate a component skeleton + accompanying `.feature` skeleton following §18.2 writing rules; dry-run unless `fs`. | needs `fs` for write |
| `i18n_extract` | Extract `qsTr` strings, return missing/stale catalog entries. | readOnly |
| `project_info` | Ljós version, enabled modules, ledger summary, MSRV, available caps. | readOnly |

**Resources and templates** (all with `ttlMs`; `cacheScope: "public"` for static):

| URI | Content |
|---|---|
| `ljos://types/{module}/{type}` | Type card (JSON + text): members, bases, default property, ledger status. Template with completion on `module`,`type`. |
| `ljos://types/{module}` | Index of types in a module. |
| `ljos://ledger` and `ljos://ledger/{module}/{type}` | Ledger (whole is large: returned as a summary + links). |
| `ljos://docs/{path}` | Ljós guide pages (authoring, binding tiers, a11y, i18n, testing). |
| `ljos://features/{path}` | A `.feature` file (as examples for step phrasing). `ljos://features/steps` is the step catalogue (~80 steps, §18.3). |
| `ljos://lints` | Lint code table with rationale and fix. |
| `ljos://project/qml-files` | Project file index with component names. |

**Prompts**

| Name | Args | Purpose |
|---|---|---|
| `write-component` | name, purpose, module | Guides: check ledger first, a11y names, write feature first (BDD), then QML, then `qml_check`. |
| `port-from-qtquick` | file | Walk `api_status` for each used type/member, list unsupported, propose replacements. |
| `add-accessibility` | file | Run `qml_lint` a11y codes, add `Accessible.*`, verify with `app_snapshot` when an app is running. |
| `localise` | file, locales | Wrap strings in `qsTr`, run `i18n_extract`, produce catalog diffs. |
| `write-feature` | behaviour | Gherkin per §18.2 rules, picks steps from the catalogue. |

For Ljós's own development, the same server is used with `--caps run,fs`, plus project-only tools (`ledger_coverage`, `oxc_check`) registered when `--project` is the ljos-ui repo. These live in `ljos_mcp_author` behind the `dev-repo` feature.

### 3.4 Live inspector — in-app (Streamable HTTP)

Launch: `ljos run --mcp` or app code `ljos_dev::mcp::attach(&engine, McpConfig{caps, bind})`. Prints one line `mcp: http://127.0.0.1:PORT/mcp (token in target/ljos/mcp-PID.json)` to stderr. Ephemeral port by default.

| Name | Semantics | Cap |
|---|---|---|
| `app_status` | App/window state: Ljós version, generation, windows (size, scale, appearance), focused ref, visibility state per window (Active/Occluded/Hidden/…), `clock: real|virtual`, caps granted. | inspect |
| `app_snapshot` | Text outline of the a11y tree (+ structured mirror) with refs; params `root?` (ref/locator), `depth`, `include: [bounds, states, objectName, ids]`, `max_nodes`, `cursor`, `image?`, `save?`. Returns `snapshot_id`/epoch, `truncated`, `next_cursor`, plus **delta** vs the previous epoch when `since` given. | inspect |
| `app_find` | Query nodes by `role`, `name` (exact/substring/regex), `objectName`, `text`, `state`; returns ≤N matches with refs. The cheap alternative to a full snapshot (Playwright `browser_find`). | inspect |
| `app_inspect` | One node in depth: type, `ObjectKey`, QML source location, properties (with values and **provenance**: assigned / bound-tier0/1/JS / animated / default), geometry, effective visibility and lattice state, a11y props. | inspect |
| `app_binding_graph` | Dependency graph for `{ref, property}`: sources and dependents (depth-limited), last-evaluation reason; pairs with `binding_tier_explain`. | inspect |
| `app_get_property` / `app_set_property` | Read/write a property by `{ref|locator, name}`. Set marks provenance as "assigned by devtools". | inspect / mutate |
| `app_invoke` | Emit signal or call an invokable method with JSON args (`mutate`). | mutate |
| `app_click` | Click a ref/locator, via the *same `InputRouter` path as kittest* (hit-test, focus, pointer events), not by calling handlers directly. Params `button`, `count`, `modifiers`, `at?` offset. Returns brief delta. | input |
| `app_type` | Insert text into focused or targeted item through `TextInputClient` (IME-equivalent); `clear?`, `submit?`. | input |
| `app_press` | Key chord (`Enter`, `Ctrl+S`) through `FocusTree`/Shortcut dispatch. | input |
| `app_scroll` | Wheel/touch scroll on a container or scroll-to-item (materialises virtualised rows). | input |
| `app_touch` | Tap/swipe/long-press/pinch gesture (mobile-style). | input |
| `app_wait_for` | Wait until all conditions hold: locator present/absent, property equals, text contains, `reload_generation ≥ N`, `idle` (no scheduled work), N virtual ms advanced. Timeout default 5 s, max 30 s. | inspect |
| `app_advance` | Advance the virtual `Clock` N ms (only with `--test-clock`). | input |
| `app_screenshot` | PNG of window or ref, rendered by the `vello_cpu` reference renderer so it works when occluded/headless; `scale`, `max_px`. | inspect |
| `app_resize` / `app_set_appearance` | Window size/DPI/orientation presets; light/dark, contrast, reduced motion, font scale (`SystemPrefs`). | input |
| `app_reload` | Force recompile+reload (normally the watcher does); returns patch summary or errors. | mutate |
| `app_alias` | Import a module from another directory in the project, or drop that alias, and reload keeping state; returns the generation. (Added after this report; see ARCHITECTURE.md §19.4.) | mutate |
| `app_logs` | Tail structured log/`console.*`/JS exceptions with QML file:line, level filter, `since` cursor; ≤100 lines default. | inspect |
| `app_metrics` | Last-N `FrameReport` summaries: per-lane time, top `ProcId` owners, bytes uploaded, draw calls, node counts, memory vs §8.5 budgets, queue depths of `UiInbox`. | inspect |
| `app_eval` | Evaluate a JS expression in the engine context with a slice deadline; result JSON ≤64 KB wrapped as `{value}`. | eval |
| `app_batch` | Array of steps (`click`/`type`/`press`/`wait_for`/`find`) executed in one response, stop-on-error; eguidev-style round-trip reduction without a scripting language. | per-step cap |
| `app_record_start/stop` | Optional: frame-sequence/GIF recording; M4. | inspect |

Naming alignment: `status/snapshot/click/type/press/scroll/wait_for/resize/set_appearance/recording_*/evaluate` map 1:1 to HAL-C2's `preview_*` verbs (`evaluate`→`app_eval`).

**Resources (inspector)**: `ljos://app/tree` (latest snapshot; subscribable → updates on reload/tree structure change), `ljos://app/logs`, `ljos://app/metrics`, `ljos://app/source/{path}` (the QML as loaded, with current generation), plus the same `ljos://types/...` and `ljos://ledger` as the authoring server so one connection suffices.
**Prompts (inspector)**: `debug-binding` (walk provenance and graph), `fix-layout` (screenshot + snapshot with bounds), `a11y-walkthrough` (tab-walk with `app_press Tab` and check names).

**Snapshot format** (the text mirror; one line per node, 2-space indent):

```
- window "Ljós Chat" [ref=e1] 1280x800
  - toolbar [e2]
    - button "New chat" [e3] id=newBtn
    - textfield "Search" value="" [e4] focused
  - list "Messages" [e5] items=240 visible=12..24
    - listitem "Alice: hi" [e6] pos=13/240
    - button "Send" [e9] disabled
```

Rules: role = AccessKit role name; quoted name = accessible name; `id=`/`obj=` show `id` or `objectName` when present (also usable as locators `#newBtn`); states are bare words (`focused`, `disabled`, `checked`, `busy`); virtual lists show window and `size_of_set` and do not materialise all rows; names clipped at 200 chars; hidden/frozen subtrees summarised as `(hidden: 18 nodes)`. Default budget ≈ 400 nodes / 40 KB, `next_cursor` for the rest, `depth` and `root` for focusing.

**Locator dialect** (so that HAL-C2's `locator` can pass through): `role=button[name="Send"]`, `text=Continue`, `#objectName`, `ref=e9`, chained with `>>` (Playwright-style), `nth=` for disambiguation, `name~=regex`. We deliberately support a *subset* of Playwright role selectors, so HAL-C2's existing prompts and agent habits transfer. CSS `selector` is not supported (error says so).

**Error model.** Tool-level `isError:true` with a short code: `stale_ref`, `not_found`, `ambiguous (N matches)`, `busy` (inbox full — retry), `not_actionable (disabled/hidden/off-screen)`, `timeout`, `capability_denied (needs input)`, `app_faulted (panic/JS interrupt; last_fault attached)`. Failures reuse `Fault` kinds from §9.3. Actions never silently fall back to coordinates.

### 3.5 Internal structure

```rust
// ljos_mcp_core
pub trait McpSurface: Send + Sync + 'static {
    fn tools(&self) -> &[ToolDef];                 // deterministic order
    async fn call(&self, cx: &CallCx, name: &str, args: Value) -> ToolOutcome;
    fn resources(&self) -> &dyn ResourceProvider;
    fn prompts(&self) -> &[PromptDef];
}
pub struct CallCx { caps: CapSet, token_id: TokenId, budget: ResponseBudget, trace: TraceCtx }
```

- `ToolDef`: JSON Schema from `schemars`, `required_cap`, `annotations`, `max_result_bytes`.
- The rmcp glue (`#[tool_router]` or dynamic `ServerHandler::call_tool`) is one file; because tool lists are cap-dependent, prefer a **dynamic registry** over compile-time macros for the inspector (list varies by authorization only, which the spec permits), and use the macros for the fixed authoring server.
- Inspector side: `DevReq { id, kind, reply: oneshot::Sender<DevReply>, deadline }`. UI-side handler returns `JobPoll::Yield` for large trees; `deadline` expiry returns a partial result flagged `truncated`.
- Input actions return only after the next frame has been produced (so the delta reflects the effect), bounded by 2 s real or a settle condition (`idle`).

### 3.6 HAL-C2 alignment: connect directly, adapt thinly

Options:
1. **HAL-C2 proxies/aggregates** Ljós's MCP tools into its own server: duplicates schemas, doubles tool lists, adds a hop and a second auth layer.
2. **HAL-C2 connects directly and re-exposes through `preview_*`** (recommended): HAL-C2's `PreviewAutomationBroker` already abstracts the target (browser tab). Add a *Ljós target* implementation that is an MCP *client* (rmcp client or TS SDK) to the app's inspector, mapping `snapshot→app_snapshot`, `click{locator}→app_click`, etc. HAL-C2 keeps owning thread-bound tokens, the inline preview panel, recordings (it can ingest `app_screenshot` frames), and capability gating (`preview`).
3. Agents talk to the Ljós inspector directly (config in the project's `.mcp.json`) — fine for Claude Code on Ljós itself; HAL-C2 can inject the URL/token into the agent environment via the project's environment variables (similar to how `environment-port` targets work for dev servers).

Recommendation: **(2) for HAL-C2 threads, (3) for plain Claude Code/Cursor use.** Mapping notes:
- `preview_snapshot.accessibilityTree` ← our structured mirror; `interactiveElements` ← filter by actionable roles with `selector` filled by a locator string; `consoleEntries` ← `app_logs`; `networkEntries` ← optional tokio-service log; `screenshot` ← `app_screenshot` (PNG, same shape); `actionTimeline` is HAL-C2's own.
- `preview_open` for Ljós = launch (`ljos run --mcp`) or attach to a `target: {kind:"ljos", url, token}`; the `environment-port` pattern extends.
- `preview_resize` presets and `preview_set_appearance` map to `app_resize`/`app_set_appearance`. `preview_navigate` has no analogue (QML has no URL); map to `Loader` source/StackView push only if an app exposes it; otherwise return an "unsupported" error.
- `preview_evaluate` ↔ `app_eval` (JS in QML context, not page JS). Document the difference in the description; gate with the `eval` cap.
- Keep Ljós's own snapshot ≤ 40 KB so HAL-C2's 60 KB wrapper and the 25k-token ceiling still hold after adding console/logs.
- Mobile: HAL-C2 `device_*` + `agent-device` can drive a Ljós Android app through the platform a11y tree already (AccessKit Android adapter 0.9.0 is young); our inspector is the richer path. The adapter selects `app_*` when the inspector is reachable and falls back to `agent-device` otherwise.
- Version negotiation: HAL-C2's server is TS (`effect/unstable/ai`); its Streamable HTTP server uses bearer tokens per thread. Same shape as our token, so the adapter just passes `Authorization: Bearer`.

### 3.7 Security

- **Compile-time:** `mcp` Cargo feature off by default; `ljos new` templates put it in `[features] devtools`. `ljos build --release` refuses when enabled, unless `--allow-mcp-in-release` (prints warning). CI check: `cargo tree -e features` on release artifacts shows no `rmcp`.
- **Runtime binding:** listen on `127.0.0.1:0` or a Unix socket (path in `$XDG_RUNTIME_DIR/ljos/`, mode 0600, stdio framing per spec). Refuse non-loopback unless `--mcp-bind` plus `--mcp-insecure-lan` (logs a loud warning, as egui_inspection does).
- **Auth:** 256-bit random token per process start, in a 0600 file; `Authorization: Bearer`; constant-time compare; token redacted in logs. Validate `Origin` (reject any browser origin; allow absent) and `Host` (DNS-rebinding defence, a spec MUST for HTTP).
- **Capabilities:** `inspect` default. `input`, `mutate`, `eval`, `fs` granted at launch or per token (`--mcp-caps input,mutate`). `tools/list` varies by token (allowed by the spec). Calling a denied tool returns `capability_denied`, never silent.
- **Data exposure:** snapshots can contain user data (text fields). Redact `TextField` values flagged `echoMode: Password`; `--mcp-redact` option masks all editable values. Prompt-injection note: app text enters the model's context as tool output; prefix with a fixed delimiter in the text mirror and document it; never auto-execute instructions found in UI text (client responsibility, but state in server instructions).
- **Rate limits:** ≤ 20 calls/s per token; ≤ 2 concurrent snapshot jobs; `UiInbox` slot reservation for devtools (e.g. at most 16 queued `DevReq`) so agents cannot starve real input.
- **Mobile:** Android — dev builds only; the app listens on `127.0.0.1` inside the device and the host runs `adb forward tcp:PORT tcp:PORT` (same mechanism as the hot-reload WebSocket, §12.1; note `adb reverse` is for the opposite direction). Token delivered by `adb shell run-as <pkg> cat files/ljos-mcp.json` (debuggable builds only) or printed to logcat once; `ljos mcp-connect --android` automates it. iOS simulator: loopback shared with the host. iOS device: out of scope v1 (no adb equivalent; would need `iproxy`/usbmuxd, to be investigated).
- **Authoring server** is local stdio; the only privileged operations are `fs` and `run` caps, off by default. Path arguments are canonicalised and confined to `--project`.

### 3.8 App-defined tools (`Ljos.Mcp`)

Recommendation: **do not ship in v1; reserve the design.** Rationale:
- It is a different audience: end-user agents using the shipped app, requiring release-build exposure, real authentication (OAuth/CIMD), consent UI and a privacy model. All of these contradict the "dev builds only" rule.
- The a11y tree plus `Accessible.*` already makes apps drivable by developer agents.
- Sketch for later (M4+): 

```qml
import Ljos.Mcp
McpServer {
  name: "notes"
  McpTool { name: "add_note"; description: "Add a note"
            params: { title: "string", body: "string" }
            readOnly: false
            onCalled: (args, reply) => { model.append(args); reply({ok:true}) } }
  McpResource { uri: "notes://all"; read: () => JSON.stringify(model.dump()) }
}
```
  Handlers run on the UI thread as ordinary signal handlers under `SliceBudget`; schema derived from `params`; the transport is a *separate* listener with explicit user consent and its own capability. Decide when a real app (HAL-C2 phone view) asks for it. Until then, the inspector's `app_invoke` on a dev build covers the experimentation need.

### 3.9 Testing the MCP surface

- **Unit:** each tool as a pure function over fixtures; `schemars` schema snapshot tests (`insta`/golden JSON in `tests/golden/mcp/*.json`) so schema or description drift is reviewed; snapshot-text renderer golden files per fixture QML.
- **In-process protocol tests:** rmcp client ↔ server over `transport-worker`, in `ljos_test`/`ljos_bdd`; deterministic `Clock`.
- **Conformance:** `npx @modelcontextprotocol/conformance server --url …` (package listed at 0.1.16 stable, 0.2.0 alpha builds exist; README says unstable). Pin exact version in a mise task `mcp:conformance`, run in CI against both servers (authoring served on HTTP in test mode). Treat failures as advisory at first.
- **Inspector tool:** `npx @modelcontextprotocol/inspector` for manual debugging (documented in `docs`).
- **BDD (features/tooling/mcp/*.feature):** written in QML-author language, e.g. "Given an app showing the QML below / When the agent asks for a snapshot / Then the snapshot lists a button named 'Save'". Steps call the MCP client via a new `Backend::mcp(tool,args)` step set; the world hosts the inspector in-process. Tags `@ljos-only @headless`, plus `@api-` not applicable. Scenario areas: snapshot contents/truncation/pagination, stale ref, locator ambiguity, capability denial, read-only default, input via same path as clicks (assert the QML `onClicked` fired and focus moved), reload preserves durable locators, idle CPU unchanged (assert no scheduled wakeups after a request completes), size caps, token/Origin rejection, redaction of password fields.
- **Perf gate:** `bench` adds snapshot of 10k-node scene ≤ 2 frame budgets amortised via Yield, and zero allocations/threads when no client attached.
- **Agent evals (nightly, optional):** a small task suite ("fix this unnamed button", "port this Qt snippet") run with Claude Code against `ljos mcp`, measuring tool calls and tokens. Tracks regressions in tool ergonomics.

### 3.10 Milestones and budgets

| Milestone | Deliverable |
|---|---|
| **M0** | `ljos_mcp_core` + `ljos mcp` with `qml_check`, `qml_lint`, `type_lookup`, `api_status`, `project_info`, resources `ljos://types/*`, `ljos://ledger`, `ljos://docs`; stdio only; golden tests; `.mcp.json` in repo so Claude Code working on Ljós gets it immediately. In-process `transport-worker` test harness. |
| **M1** | `qml_format`, `binding_tier_explain`, `docs_search`, `features_list/run`, `run_status`, prompts; inspector read-only: `app_status/snapshot/find/inspect/screenshot/logs/metrics`, token + loopback HTTP; BDD feature dir `tooling/mcp`. |
| **M2** | Input and mutation caps (`app_click/type/press/scroll/set_property/invoke/wait_for/batch/reload/eval`), binding graph, `subscriptions/listen` for reload events, **HAL-C2 Ljós preview target adapter**, `qt_compare`, conformance in CI. |
| **M3** | Android: `adb forward` flow + `mcp-connect --android`; touch gestures; mobile redaction. |
| **M4** | Recording, tasks extension, `component_scaffold`/`i18n_extract` polish, decision on `Ljos.Mcp`, release-build guard hardening. |

**Budgets**
- Tool list: ≤ 25 tools per server by default (authoring ≈ 16, inspector ≈ 21 at `inspect+input`); each description ≤ 300 chars; slim profile `--mcp-profile=slim` ≈ 8 tools (`snapshot, find, click, type, press, wait_for, screenshot, logs`).
- Responses: 40 KB default, 80 KB hard cap text; snapshot default 400 nodes; names ≤ 200 chars; logs ≤ 100 lines; eval ≤ 64 KB; screenshots ≤ 1280 px long edge, PNG ≤ 1 MB (else downscale).
- Latency (desktop): `app_find` p95 < 20 ms, 500-node snapshot < 40 ms UI-thread time spread over ≤ 2 frames; `app_click` returns within 1 frame + settle.
- Idle: zero UI-thread wakeups attributable to MCP without a request; tokio listener threads parked.
- Dependency budget: `ljos_mcp_core` ≤ ~60 transitive crates incl. tokio (measure in CI; fail on +20 %).

---

## 4. Risks and open questions

1. **Spec churn.** 2026-07-28 is brand-new (stateless rewrite, MRTR). Clients lag: some still speak 2025-11-25 and expect `initialize`. rmcp 3.x bridges both, but exact behaviours (e.g. sessionless HTTP) need testing against Claude Code, Cursor and HAL-C2's client. Mitigation: CI matrix of client versions via rmcp client with both protocol versions.
2. **rmcp breaking changes.** The 3.0 wave touched handler return types, `Meta`, tasks and OAuth. Pin `=3.5.x`, isolate behind `McpSurface`, budget quarterly upgrades. Verify the exact structured-output API (`Json<T>`/`outputSchema` generation) in 3.5.1 before the M0 spike. The facts above come from docs.rs/README summaries, not from reading the crate source.
3. **Token cost.** Tool schemas are paid every turn; trees are paid per call. Slim profile, delta snapshots, `app_find`, `app_batch`, caps on size, deterministic tool order (prompt-cache friendly). Measure real token counts on 5 reference apps in M1 and tune the 40 KB default.
4. **Claude Code result handling.** HAL-C2's code notes it rejects non-object `structuredContent` and drops >25k-token results, even though the new spec loosens both. We follow the stricter client behaviour (objects only, small outputs) until proven otherwise.
5. **Screenshots on headless/occluded windows.** Using the `vello_cpu` R renderer sidesteps occlusion failures (egui_inspection documents them) but may differ slightly from B0/B1 output. Provide `renderer: "reference" | "live"` option; document that agents shouldn't pixel-compare across them. Needs a spike to render an arbitrary `RenderTree` snapshot off the UI thread (depends on §7.2 `FrameSnapshot` being `Send`).
6. **A11y tree as the agent surface.** Quality depends on authors adding `Accessible.name`. Unnamed controls show as `button [e3] id=…` only. The lint-driven workflow (§14 audit) is the mitigation; `app_snapshot` should flag unnamed actionable nodes with `⚠unnamed`.
7. **Stable refs across virtualised delegates and reloads.** Ephemeral `eN` are per-epoch by design; durable locators rely on `objectName`/`id`/role+name and may be ambiguous in lists. `app_find` returns `ambiguous` with candidates; `nth=` helps. Open: expose model row keys as `row=KEY` locators.
8. **Security of mutation/eval.** A compromised agent or prompt-injected UI text can drive the app. Mitigations above are defence in depth; the real boundary is "dev builds, loopback, token". Open: should `eval` be disabled whenever a network service (tokio) holds credentials? Probably a per-app policy flag.
9. **Mobile.** Android `adb forward` is fine for debuggable builds; AccessKit Android/iOS adapters are young (0.9.0/0.2.1). iOS device path undefined. Port/token handoff UX needs a spike in M3.
10. **Task/long-running ops.** Tasks extension support in clients is uncertain; handle+poll fallback adds two tools. Revisit at M4.
11. **Clean-room.** `qt_compare` runs real Qt as an oracle only; docs search must index Ljós docs and Qt *documentation links*, never Qt source. Document in CLEAN_ROOM.md.
12. **Process/maintenance load.** Two servers + HAL-C2 adapter = three surfaces to keep in sync with the ledger. Single source: `ToolDef` registry generates the tool tables in docs and golden schemas.
13. **Ownership of `Ljos.Mcp`** (end-user agent access) is intentionally deferred; revisit with a real consumer, and check whether MCP Apps / extensions mature further by then.
14. **Unverified items:** exact crates.io dependency count for the rmcp server feature set; whether `rust-mcp-sdk`'s conformance claim holds; availability of a Tauri-specific MCP plugin (search returned nothing authoritative).

---

## 5. Sources

Spec and protocol
- Key changes in 2026-07-28: https://modelcontextprotocol.io/specification/2026-07-28/changelog
- Transports overview: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports
- Streamable HTTP: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http
- Tools: https://modelcontextprotocol.io/specification/2026-07-28/server/tools
- Feature lifecycle and deprecation policy: https://modelcontextprotocol.io/community/feature-lifecycle
- Release blog: https://blog.modelcontextprotocol.io/posts/2026-07-28/ (RC: https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/)
- Conformance runner: https://github.com/modelcontextprotocol/conformance (npm `@modelcontextprotocol/conformance`)

Rust SDKs
- rmcp on docs.rs: https://docs.rs/crate/rmcp/latest (features: https://docs.rs/crate/rmcp/latest/features)
- rmcp on crates.io: https://crates.io/crates/rmcp
- Repository/README: https://github.com/modelcontextprotocol/rust-sdk
- 3.x migration: https://github.com/modelcontextprotocol/rust-sdk/discussions/969
- rust-mcp-sdk 2.0.0: https://docs.rs/crate/rust-mcp-sdk/2.0.0

Prior art
- Playwright MCP: https://github.com/microsoft/playwright-mcp
- Chrome DevTools MCP: https://github.com/ChromeDevTools/chrome-devtools-mcp
- Dart/Flutter MCP server: https://dart.googlesource.com/ai.git/+/6a71aeb/pkgs/dart_mcp_server/README.md
- egui_inspection: https://docs.rs/egui_inspection
- eguidev: https://docs.rs/crate/eguidev/0.0.3 (repo: https://github.com/cortesi/eguidev)
- Godot/Unity MCP listings: https://godotengine.org/asset-library/asset/5245 and search results on ui-toolkit-mcp (not independently verified)

Local, read-only
- /home/olafura/dev/hal-c2/apps/server/src/mcp/toolkits/preview/{tools,handlers}.ts
- /home/olafura/dev/hal-c2/apps/server/src/mcp/toolkits/device/{tools,handlers}.ts
- /home/olafura/dev/hal-c2/apps/server/src/mcp/McpHttpServer.ts (auth middleware, `MAX_SNAPSHOT_TEXT_BYTES = 60_000`)
- /home/olafura/dev/hal-c2/packages/contracts/src/previewAutomation.ts (snapshot, locator contracts)
- /home/olafura/dev/ljos-ui/docs/ARCHITECTURE.md §3, §4, §9, §12, §14, §17, §18
