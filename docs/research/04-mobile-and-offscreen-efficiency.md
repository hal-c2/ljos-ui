# 04 — Mobile platforms, and resource efficiency for on-screen vs off-screen content

Status: research report for Ljós UI (QML semantics on egui, rquickjs, egui_kittest, signals/slots, tokio). Written October 2026. Clean-room: Qt/Slint were consulted through public documentation only (doc.qt.io, docs.slint.dev); no Qt or Slint source was read. Permissively licensed Rust crates (winit, android-activity, egui, wgpu, accesskit, rquickjs) were read at source level.

Items marked **[verify]** come from memory or secondary sources (Apple developer docs need JavaScript and could not be fetched) and need a check before they drive a decision.

---

## 1. Summary of key findings

**Mobile platform stack**

- **winit is not enough on Android.** In winit 0.30.13 and 0.31.0-beta.3 the Android backend maps `Resumed` ⇐ `InitWindow` and `Suspended` ⇐ `TerminateWindow` (surface lifetime, not Activity lifetime). It does *not* forward `Start/Stop/SaveState/Destroy/InsetsChanged/ContentRectChanged`. `LowMemory` becomes `memory_warning`. IME support is show/hide only. `Window::safe_area()` (0.31 beta) returns zero on Android.
- **egui/eframe 0.36.2 has no real Android safe-area or soft-keyboard story.** Safe area is iOS-only in egui-winit (Android issue emilk/egui#7748 open); native-Android keyboard is issue #5590. The 0.36 mobile keyboard fix is web-only. eframe offers `android-game-activity` / `android-native-activity` features and the 0.33 line added back-button handling; drag-to-scroll with touch exists but has no platform-native fling/overscroll.
- **android-activity 0.6.1 is the right foundation** (GameActivity preferred over NativeActivity): it exposes the whole `MainEvent` set (`SaveState`, `Pause`, `Stop`, `Destroy`, `InsetsChanged`, `LowMemory`, `GainedFocus`...), `content_rect()`, `show_soft_input`, `set_text_input_state` / `set_ime_editor_info` (GameTextInput), and `run_on_java_main_thread`. `android_main` can be entered multiple times per process.
- **wgpu 30** returns `CurrentSurfaceTexture::{Success, Suboptimal, Timeout, Occluded, Outdated, Lost, Validation}`. On Android the surface must be created on resume and dropped on suspend; the device can be kept (verify per-driver) and all surface-bound state is rebuilt.
- **AccessKit is mobile-ready enough:** `accesskit_android` 0.9.0 (`Adapter`, `InjectingAdapter` with embedded dex, live regions, sliders) and `accesskit_ios` 0.2.1. `accesskit_winit` supports Android only on GameActivity. AccessKit is a push-tree model, which fits an egui-style retained snapshot but means off-screen/frozen subtrees must still publish *something* sensible (or be omitted deliberately).
- **Platform rules that bite:** edge-to-edge is enforced for targetSdk ≥ 35; predictive back is default-on at targetSdk 36 and `KEYCODE_BACK`/`onBackPressed` stop arriving unless you opt out (`android:enableOnBackInvokedCallback="false"`, which HAL-C2's Qt app currently does) or implement the back-invoked callback via JNI/Java glue; `onTrimMemory` levels `UI_HIDDEN`/`BACKGROUND` are the useful ones (`MODERATE`/`COMPLETE` deprecated); iOS suspends apps shortly after backgrounding and kills sockets.
- **Build tooling:** Gradle + `cargo-ndk` 4.x is what android-activity recommends. `cargo-apk` is NativeActivity-only and stale, `xbuild` is unmaintained, `cargo-mobile2` (0.22.5) is Tauri-centric, Bevy defaults to GameActivity. For iOS: Xcode project (XcodeGen-style generation) linking a Rust staticlib.

**Efficiency**

- **Demand-driven rendering is the single biggest win** (idle CPU/GPU → 0). egui's model (`request_repaint`, `request_repaint_after`, `set_request_repaint_callback`) already supports it; Ljós must make *every* ticking source (animations, timers, JS microtask queues, network events) register its own wake-up and nothing poll.
- **QML semantics are mostly freeze-friendly but not entirely.** Bindings are pure-ish expressions with change notification, so a hidden subtree can have its binding *re-evaluation deferred* ("dirty-on-notify, evaluate-on-demand") without observable difference — except for bindings with side effects, `onXChanged` handlers, `Behavior`, `Connections`, and Timers. Qt itself keeps `Timer` running when invisible, so Ljós should keep that default and make suspension an explicit, opt-in policy.
- **Virtualization must be the default for long models:** pooled delegates, a cache window of roughly ±1–2 screens, time-sliced incubation (≤ ~2–4 ms per frame), reuse signals equivalent to Qt's `pooled/reused`.
- **Memory:** budgeted image cache (byte-based LRU keyed by source+sourceSize), decode at `sourceSize`, glyph-atlas cap with trim on `LowMemory`/`TRIM`/`memory_warning`, rquickjs `set_memory_limit` + `set_gc_threshold` + explicit `run_gc` on background.
- **Frame pacing:** 60 Hz fixed is wrong on 90/120 Hz devices. Use `ANativeWindow_setFrameRate`/`Surface.setFrameRate` hints and `AChoreographer`; drive animation from vsync timestamps; drop to low rate when content is static. Swappy has no maintained Rust binding; call via `ndk-sys` if needed.
- **Networking:** foreground-only live sockets with a resume cursor, push (FCM/APNs) for wake-ups; tokio runtime is fine on mobile but must be paused logically (cancel/ reconnect) on background.

---

## 2. Detailed findings

### 2.1 winit on Android and iOS

Source read: `winit/src/platform_impl/android/mod.rs` at v0.30.13 and `winit-android/src/event_loop.rs` at v0.31.0-beta.3; docs.rs platform pages.

- Mapping from android-activity `MainEvent`:
  - `InitWindow` → `ApplicationHandler::resumed` (the app should create its window/surface here). `TerminateWindow` → `suspended` (the surface **must** be dropped; the native window is gone).
  - `WindowResized` / `ConfigChanged` → `Resized` / `ScaleFactorChanged` (density derived from `Configuration`).
  - `RedrawNeeded` → `RedrawRequested`. `LowMemory` → `memory_warning`.
  - `GainedFocus` / `LostFocus` → `Focused`.
  - `Start`, `Resume` (Activity-level), `Pause`, `Stop`, `SaveState`, `Destroy`, `InsetsChanged`, `ContentRectChanged` are **dropped**. There is no hook to persist state at the moment Android asks for it, and no event telling you the IME or system bars changed the usable rect (other than inferring from `Resized`).
- Android IME in winit: `Window::set_ime_allowed` toggles the soft keyboard; there is no composition/preedit round trip and no cursor-area reporting on Android in 0.30. Hardware keyboards and soft-key events arrive as `KeyEvent` with limited `text`. For CJK/Icelandic dead keys/swipe typing this is not adequate.
- Safe area: 0.31 beta adds `Window::safe_area()`; on Android it returns zeros (as read in the beta.3 source). iOS returns real insets.
- iOS (winit 0.30.13 docs): uses UIKit through objc2; `Resumed/Suspended` map to `applicationDidBecomeActive/WillResignActive`-ish transitions; `memory_warning` maps from `applicationDidReceiveMemoryWarning`; touch events include `Force` and Apple Pencil data; you can set preferred status bar style, home indicator, and screen edges deferring system gestures. winit leaves view-controller-level concerns (keyboard layout guide, rotation lock) to extension traits.
- **Conclusion:** Ljós needs a `PlatformHost` abstraction. On desktop/iOS use winit (or a thin wrapper). On Android use our own host built directly on android-activity because the missing lifecycle/insets/IME events are the ones a QML-style app (HAL-C2 mobile) depends on.

### 2.2 android-activity 0.6.1 (and the GameActivity choice)

- Two backends: `native-activity` (no Java code needed, but a bare IME) and `game-activity` (requires a small Java/Kotlin Activity subclass and the AGDK GameActivity AAR; provides GameTextInput for a proper IME connection with composing regions, selection, and `InsetsChanged`).
- `AndroidApp` API surface that matters: `native_window()`, `content_rect()`, `config()`, `show_soft_input(bool)`, `hide_soft_input`, `set_text_input_state(TextInputState)`, `text_input_state()`, `set_ime_editor_info(...)`, `input_events_iter()` (motion/key/text), `run_on_java_main_thread(closure)`, `vm_as_ptr()`/`activity_as_ptr()` for JNI, `internal_data_path()`, `external_data_path()`, `asset_manager()`.
- `MainEvent` variants: `InitWindow, TerminateWindow, WindowResized, RedrawNeeded, ContentRectChanged, GainedFocus, LostFocus, ConfigChanged, LowMemory, Start, Resume{loader}, SaveState{saver}, Pause, Stop, Destroy, InsetsChanged, InputAvailable, TextInputEvent`.
- Gotcha: `android_main` may be called **multiple times in one process** (Activity destroyed, process kept). Any `static` global state (tokio runtime, JS runtime, caches) must be created in a way that handles re-entry — which is exactly why Ljós should separate *app core* (survives) from *UI host* (dies with Activity). See §3.2.
- Version note: crate at 0.6.1 as of this research; GameActivity AAR version should be pinned to the AGDK version the crate vendors.

### 2.3 egui / eframe 0.36.2 on mobile

- eframe features: `android-game-activity`, `android-native-activity` (read from `crates/eframe/Cargo.toml`). eframe's Android path goes through winit, inheriting every limitation in §2.1.
- egui-winit: `safe_area` plumbing only for iOS; Android tracked in #7748. Software keyboard: egui does not request/hide soft keyboard on Android reliably (#5590); 0.36 web mobile keyboard fix does not help native.
- Input: touch → pointer emulation, `MultiTouchInfo` (zoom/rotation/translation) available through `InputState::multi_touch()`. egui's scroll area has touch dragging with simple momentum; no per-platform fling curve, no overscroll glow/stretch.
- Repaint control: `Context::request_repaint()`, `request_repaint_after(Duration)`, `request_repaint_after_for(Duration, ViewportId)`, `set_request_repaint_callback`. egui is natively a *immediate-mode full-redraw-per-frame* system — each frame re-runs the UI closure; "demand-driven" means we skip frames, not that we skip work within a frame. That is exactly the tension with a QML-style retained binding graph (see §2.9).
- 0.33 added Android back-button support in eframe (KEYCODE_BACK path) — which silently stops working under predictive back at targetSdk 36 unless opted out.

### 2.4 wgpu surface lifecycle

- wgpu docs (`documentation::platforms::android`): create the surface in `resumed`, drop it in `suspended`; on Android the `ANativeWindow` is invalid after `TerminateWindow`.
- `Surface::get_current_texture()` in wgpu 30 returns `CurrentSurfaceTexture`:
  - `Success(tex)` render.
  - `Suboptimal(tex)` render, then reconfigure.
  - `Timeout`, `Occluded` skip frame (don't spin: wait for next event).
  - `Outdated` reconfigure with the current size and retry next frame.
  - `Lost` recreate the surface (and on some drivers the device).
  - `Validation` bug; log loudly.
- `SurfaceConfiguration::desired_maximum_frame_latency` (default 2): setting 1 trades throughput for lower input latency; on mobile 2 is usually right for battery.
- Device loss: handle `Device::set_device_lost_callback`; in the common Android case the device survives surface loss, but GPU reset or process-level eviction needs a full re-upload path (textures, atlas, pipelines). Design resources as *recreatable from CPU-side data* from the start.

### 2.5 Accessibility bridges

- `accesskit_android` 0.9.0: `Adapter` (you own the View and forward `AccessibilityNodeProvider` calls) and `InjectingAdapter` (injects a delegate into an existing View, shipping a small dex). Supports live regions and slider semantic nodes in this version. `accesskit_ios` 0.2.1 exists but is young. `accesskit_winit` on Android only works with the GameActivity path.
- Since Ljós uses its own Android host, we integrate `accesskit_android::InjectingAdapter` directly onto the GameActivity surface view, keeping us independent from winit's policy.
- Model implication: tree updates are pushed on change. For frozen/hidden subtrees, either (a) mark them `hidden` in the tree, or (b) keep their last-known nodes — never let them vanish while still logically visible to TalkBack (focus jumps).

### 2.6 Android platform behaviours

- **Edge-to-edge** is mandatory for targetSdk ≥ 35: window content is drawn under system bars/cutouts; apps need `WindowInsets` (systemBars, displayCutout, ime). GameActivity forwards `InsetsChanged`; Ljós's host must translate to `SafeArea.margins` + `keyboardRectangle` equivalents.
- **Predictive back** (`targetSdk` 36 default): `onBackPressed()` and `KEYCODE_BACK` no longer delivered; need `OnBackInvokedCallback` (API 33+) registered from Java glue, forwarding to Rust. HAL-C2 sidesteps with `enableOnBackInvokedCallback="false"` – Ljós should support both modes: legacy (works today) and callback with progress events (`BackEvent` progress for animations) later.
- **Orientation/config changes:** HAL-C2's manifest lists a long `configChanges` set so the Activity is not recreated on rotation/density/font scale. Ljós should adopt the same stance (handle config changes in-process) but survive recreation anyway (Activity can still be destroyed by the OS).
- **Memory callbacks:** `ComponentCallbacks2.onTrimMemory`: `TRIM_MEMORY_UI_HIDDEN` (all UI hidden: free GPU/UI caches), `TRIM_MEMORY_BACKGROUND` (process in LRU list). `RUNNING_MODERATE/LOW/CRITICAL` are about foreground pressure. `MODERATE`/`COMPLETE` are deprecated.
- **Doze/App Standby:** network access deferred for backgrounded apps; only FCM high-priority messages wake the app. Foreground services are for user-visible long-running work; not applicable to a chat UI.
- **Frame rate:** `Surface.setFrameRate(rate, FRAME_RATE_COMPATIBILITY_DEFAULT/FIXED_SOURCE, CHANGE_FRAME_RATE_ONLY_IF_SEAMLESS)` and `ANativeWindow_setFrameRate` (API 30+), `AChoreographer_postFrameCallback64` / `AChoreographer_registerRefreshRateCallback` (API 30+/33+) for refresh-rate changes. Android games docs recommend Swappy (`libswappy`) for frame pacing; there is no maintained Rust binding (crates.io `swappy` is an unrelated package). We would call the C API through our own thin `-sys` or avoid it initially.
- **Scroll physics:** Android uses `OverScroller` (spline-based fling, friction 0.015-ish `ViewConfiguration.getScrollFriction()`), and on Android 12+ a stretch overscroll effect. Min/max fling velocities come from `ViewConfiguration`. Velocity tracking is by `VelocityTracker`.

### 2.7 iOS behaviours **[verify]**

- Apps are suspended within seconds of backgrounding; open TCP/WebSocket connections are torn down; background `URLSession` supports downloads/uploads but not WebSockets. Apple DTS forum threads (716118, 106838, 706740) confirm sockets die on suspension. Strategy: push notifications (APNs) + resume cursor.
- Metal rendering from the background is not allowed (GPU work submitted in background gets the process killed on older iOS). Stop the display link on `willResignActive`.
- ProMotion: `CADisplayLink.preferredFrameRateRange` (CAFrameRateRange). On iPhones the `CADisableMinimumFrameDurationOnPhone` Info.plist key is required for >60 Hz in some code paths **[verify]**.
- Safe area: `UIView.safeAreaInsets`/`safeAreaLayoutGuide`; keyboard via `UIKeyboardWillChangeFrameNotification` or `keyboardLayoutGuide` (iOS 15+).
- Scroll: `UIScrollView.decelerationRate` normal 0.998 per ms; rubber-band overscroll. iOS and Android feel differs; scroll physics must be per-platform data (see §3.8).

### 2.8 How Qt documents mobile and off-screen behavior (docs only)

- **ListView:** `cacheBuffer` (pixels of extra delegates beyond the view), `reuseItems` (delegate pooling, with `ListView.pooled` / `ListView.reused` signals), `displayMarginBeginning/End`, asynchronous incubation via `DelegateModel`/`Loader`. `delayRemove`, `highlightRangeMode` and so on are about behavior not efficiency.
- **Loader:** `active: false` unloads; `asynchronous: true` incubates in the background; `status`/`progress`. The canonical lazy pattern is `Loader { active: SwipeView.isCurrentItem || SwipeView.isNextItem || SwipeView.isPreviousItem }`—the SwipeView docs recommend that for pages.
- **StackView:** items below the top are `StackView.Inactive` (invisible but kept alive) unless popped; `status` allows the app to pause work.
- **Item.visible:** invisibility propagates to children; invisible items skip rendering and input but **bindings, animations that are not scene-graph driven, and Timers continue**. `ItemVisibleHasChanged` is how C++ code observes it. `Item.layer.enabled` renders a subtree to a texture (useful for freezing a rasterized snapshot, with memory cost).
- **Image:** `sourceSize` decodes at reduced size, `asynchronous`, `cache`, `retainWhileLoading`. Qt's pixmap cache is shared and size-limited.
- **Render loops:** Qt Quick's threaded render loop is the default on most platforms (incl. Android/iOS), with sync stage on the GUI thread; the scene graph batches geometry and renders only on change (`update()`), i.e. Qt is *already* demand-driven.
- **Lifecycle:** `Qt.application.state` (`ApplicationSuspended`, `ApplicationHidden`, `ApplicationInactive`, `ApplicationActive`), plus `QGuiApplication::applicationStateChanged`. On Android these map to Activity pause/stop; the docs warn that on mobile the app may be killed in `Suspended`, so save state there.
- **Safe area:** `SafeArea` attached type (Qt 6.9+), `Qt.inputMethod.keyboardRectangle`, `Qt.inputMethod.visible` — these are exactly the APIs HAL-C2 uses.
- **Flickable:** `boundsBehavior`, `flickDeceleration`, `maximumFlickVelocity`, `rebound` transition — platform defaults differ by style.

### 2.9 What HAL-C2's mobile-qt app needs from Ljós

Read-only inspection of `apps/mobile-qt` (README, CMake, AndroidManifest, `MobileShell.qml`, `MobileApp.cpp`, `docs/internals/mobile-qt.md`):

| Need (as used today) | Ljós requirement |
|---|---|
| `Qt.inputMethod.keyboardRectangle` to lift composer | `Ljos.InputMethod.keyboardRectangle` (+ `visible`, animated `animating` hint) from `WindowInsets.ime` |
| `SafeArea.margins.{top,left,right,bottom}` | `SafeArea` attached type fed by system bars + cutout + (optionally) ime |
| `Shortcut { sequences: ["Back"] }` | Back as a first-class `StandardKey.Back` / `Keys.onBackPressed`; works with and without predictive back |
| `roomy` computed from `width/zoom`, `fullHeight/zoom` | Width/height in dp with user font-scale/zoom; re-layout on config change without Activity restart |
| `Loader { active: layout === "phone" }` | `Loader` with `active`, `asynchronous`, `sourceComponent`; frozen/inactive semantic |
| `QQuickStyle::setStyle("Basic")` | A style-selection hook; mobile styles are simple, no native widgets |
| `QDesktopServices` URL handler `hal-c2://pair` | Deep-link delivery (`intent-filter` VIEW → Rust) incl. cold start and `singleTask` `onNewIntent` |
| `launchMode="singleTask"`, long `configChanges`, `adjustResize` | Host handles config changes; `adjustResize` replaced by explicit insets handling under edge-to-edge |
| QR-pair flow (camera) | Out of scope for core UI, but plugin/host extension point for camera + permission prompts (`rxing`/`rqrr` 0.9.3/0.11.0 for decoding) |
| Streaming message lists with long histories | Virtualized `ListView` with `reuseItems`, `positionViewAtEnd`, stable scroll anchoring on prepend |
| Network (WS/SSE to desktop host) | Foreground-only socket policy, reconnect with resume cursor (§3.9) |

### 2.10 Prior art summary

- **Flutter:** `TickerMode(enabled: false)` stops tickers of subtree (route not visible); `Offstage` hides *without* stopping animations (documented CPU burn) — the same trap as QML `visible:false`; `AutomaticKeepAlive` keeps list children alive on request; lazy builders (`ListView.builder`) with `cacheExtent` (default 250 px); `RepaintBoundary` isolates layers. Lesson: offer *separate* switches for "don't paint", "don't tick", and "don't keep alive".
- **Jetpack Compose:** `collectAsStateWithLifecycle` stops collecting flows below `STARTED`; `LazyColumn` with `beyondBoundsLayout`/prefetch; `drawWithCache` caches draw objects; state reads are tracked so only readers recompose — analogous to binding dependency tracking.
- **SwiftUI:** `LazyVStack/LazyVGrid`; `.task` is cancelled on disappear; `scenePhase` environment value.
- **React Native `FlatList`:** `windowSize` 21 (10 screens each side), `initialNumToRender` 10, `maxToRenderPerBatch` 10, `updateCellsBatchingPeriod` 50 ms, `removeClippedSubviews`; they trade blank flashes during fast scrolls for memory — exactly the knob we need to expose.
- **Browsers:** Chrome throttles hidden-page timers to ≥1 s, intensive throttling to once per minute after 5 minutes hidden; `requestAnimationFrame` is paused for hidden documents; `IntersectionObserver` for visibility; CSS `content-visibility: auto` skips layout/paint of off-screen subtrees with `contain-intrinsic-size` placeholders — this is the closest model to a "frozen subtree".
- **Qt Quick:** as §2.8 — keeps hidden items fully live; efficiency is delegated to the developer via Loader/ListView patterns.

### 2.11 rquickjs on mobile

`rquickjs 0.14`: `Runtime::set_memory_limit(bytes)`, `set_gc_threshold(bytes)`, `run_gc()`, `memory_usage()`, `set_interrupt_handler(Fn() -> bool)` (called periodically; return true to abort). Uses: cap JS heap per app (e.g. 64–128 MiB on phone), interrupt handler as a *time budget* (deadline check), `run_gc()` on `Stop`/`UI_HIDDEN`, and drop the JS context entirely only if app-state can be rebuilt (it cannot, in general — don't).
Store policy note: Google Play permits interpreted code that has only indirect access to Android APIs, provided it can't enable policy violations (Device and Network Abuse policy); Apple's 2.5.2 allows interpreted code that doesn't change the app's primary purpose **[verify]**. A QML app whose QML/JS ships in the bundle is clearly fine; downloading QML at runtime is the risky case.

### 2.12 Test and tooling notes

`egui_kittest` (AccessKit-based `Harness`, `get_by_label`, `snapshot`) can run headless on desktop, so lifecycle and visibility policies should be designed as *pure state machines with injectable events* (`Event::Suspend`, `Event::Insets(..)`, fake clock) to be testable without devices. Crate versions checked on crates.io (Oct 2026): rustls-platform-verifier 0.7.1, tokio-tungstenite 0.30.0, reqwest 0.13.5, jni 0.22.4, ndk-sys 0.6.0, ndk-context 0.1.1 (stale), rxing 0.9.3, rqrr 0.11.0.

---

## 3. Concrete recommendations for Ljós

### 3.1 Own the host: `PlatformHost` trait

**Decision:** define `trait PlatformHost` (events in, capabilities out). Implementations: `WinitHost` (Linux/macOS/Windows/iOS, wrapping winit 0.31), `AndroidHost` (android-activity 0.6 + GameActivity), later `WebHost`.
Event vocabulary (platform-neutral): `SurfaceCreated/Destroyed`, `Resized{px, scale}`, `Insets{system_bars, cutout, ime}`, `Focus`, `AppState{Active, Inactive, Hidden, Suspended}`, `MemoryPressure{Level}`, `Back{phase, progress}`, `Text{composing, selection}`, `Touch/Pointer/Key`, `OpenUrl`, `SaveState request`.
**Trade-off:** more code than "just eframe", but eframe/winit on Android lose Stop/SaveState/Insets/IME. **Rejected:** forking winit's Android backend (maintenance burden, upstream 0.31 churn); relying on eframe's `android-*` features (blocks exactly the HAL-C2 needs).

### 3.2 Split "app core" from "UI host" (survive Activity death)

Android can destroy the Activity and keep the process. Keep the QML object tree, JS runtime, tokio runtime and models in an **app core** whose lifetime is the process; the UI host (egui `Context`, wgpu surface, glyph atlas, GPU textures) is rebuilt on `SurfaceCreated`. After `Destroy` + process kill, restore from a **UI-state snapshot** (`Bundle`-sized, ≤ ~100 KiB): current route stack, scroll anchors (model key + offset), text field drafts, selected tab. QML API: `Ljos.Persist { id: "thread.draft"; property: ... }` (opt-in), saved in `SaveState`/`Stop` (prefer writing on `Stop`, since `SaveState` size limits are tight and winit can't give it to us anyway).
**Rejected:** relying on `android:configChanges` alone (does not help with process death); serializing the full object tree (fragile, QML objects hold arbitrary JS state).

### 3.3 Demand-driven frame scheduler

- A single `FrameScheduler` owns the "next wake" time: `min(animation deadlines, timer deadlines, request_repaint_after, network events)`; the event loop sleeps otherwise. Budget: **idle = 0 wakeups/s, 0 % CPU**; a blinking text cursor is 2 wakeups/s max and is suppressed when the window is not focused/visible.
- Every animation registers an `AnimationClock` with the scheduler; when all animations stop, no frames are scheduled. JS `setTimeout`/`Timer` also register deadlines (coalesced to ≥ 1 ms granularity, ≤ 4 ms slack allowed for non-animation timers to batch wake-ups).
- On Android run animation off `AChoreographer` vsync timestamps (not wall-clock sleeps). Frame budget: **16.6 ms @60 Hz, 8.3 ms @120 Hz**; target ≤ 50 % of the budget CPU on a mid-range device (≈ 8 ms / 4 ms) to leave thermal headroom.
- Immediate-mode caveat: egui re-runs the full UI closure every painted frame. To keep cost proportional to *change* not *tree size*, Ljós's render pass should paint from a cached retained display list per subtree and only call egui for dirty regions/widgets — measure first; don't pre-build (see §4).

### 3.4 Visibility lattice and suspension policies

Per-node state computed top-down, cheap to propagate on change only:

`Active` (visible, window focused) › `VisibleUnfocused` (visible, window not focused: multi-window/split-screen) › `Hidden` (`visible:false`, offscreen in a viewport, covered route) › `Suspended` (app state Hidden/Suspended) › `Frozen` (opt-in deep freeze).

Defaults preserve **QML semantics**: `visible: false` removes from paint/input/layout-size-contribution as in Qt, but bindings/timers/behaviors keep running. Efficiency comes from *opt-in or structural* policies:

1. **Paint/layout culling (always on):** nodes outside the viewport clip or `visible:false` skip layout-dependent painting; no per-frame cost beyond a rect test (use spatial culling at container level, e.g. `Flickable` knows its content window).
2. **App-state suspension (always on):** when the app is `Hidden/Suspended` stop the frame scheduler, stop animations' clocks (they resume in *logical time* — see below), release the surface.
3. **`Timer.suspendWhen: Timer.Hidden` (opt-in policy, default `Never`):** values `Never | Hidden | AppInactive`. Matches the user's note that a Timer suspension policy must be opt-in. When suspended, the timer does not fire and does not accumulate missed ticks (`triggeredOnStart` and `repeat` semantics unchanged on resume).
4. **Animations:** run-on-visible is the default for *scene animations* (they cost frames only if painted — a purely visual `NumberAnimation` on a hidden item need not tick: jump to the final value when it would have completed, preserving `running`/`finished` signals). Animations with observable side-effects (`onFinished`, `ScriptAction`, property changes read by bindings elsewhere) use logical-time catch-up: advance analytically without painting. **Rejected:** Flutter-style "Offstage keeps ticking" and Qt-style fully live hidden animations (both burn CPU, a documented Flutter pitfall).
5. **`Frozen` (opt-in per subtree, auto for covered `StackView` pages and `SwipeView` pages beyond ±1):** see §3.5.
6. **Lifecycle-aware sources:** `Ljos.Lifecycle` attached property (`state`, `onStarted`, `onStopped`) and `Connections`-like `enabled: Lifecycle.started` for event sources such as WebSocket subscriptions (equivalent of Compose's `collectAsStateWithLifecycle`).

### 3.5 Freezing off-screen subtrees — QML semantics analysis

Goal: stop *work*, not *meaning*. Rules for a `Frozen` subtree:

- **Binding evaluation becomes lazy:** dependency notifications mark bindings *dirty* (O(1)); re-evaluation happens when (a) the property is read, (b) the subtree thaws, or (c) a handler/`Connections` observing it requires the change signal. **Safe** when binding expressions are pure (Ljós already forbids side effects in binding expressions by convention; QML does not enforce). **Unsafe cases** to detect and exempt: bindings that call functions that mutate state; `on<Property>Changed` handlers (they require eager evaluation to fire — freeze must either evaluate eagerly or *coalesce* to a single change signal on thaw — choose: **coalesce, documented**, because intermediate values are unobservable by definition when frozen); `Behavior` and `Binding {}` element with `when:`; `Connections` targets outside the subtree.
- **Layout:** frozen items keep their last geometry and `implicitSize`; size-affecting dirty marks are deferred, but if a frozen item sits inside a visible layout whose result depends on its implicit size, it is *not* frozen (only subtrees whose outer size is independent of the content — e.g., clipped route pages, off-screen delegates — qualify). The freeze analysis is a static check on parent type: `StackView/SwipeView/Loader/ListView delegate` yes; plain `ColumnLayout` child no.
- **Rendering:** no painting. Optionally keep one cached texture (`layer`-like snapshot) for transitions/predictive-back previews; budget ≤ one screen-size RGBA8 (~8–12 MiB at 1080×2400 = 10.4 MiB) per cached page and at most 2 pages.
- **Accessibility:** frozen = `hidden` in the AccessKit tree unless the page is the active route; avoids TalkBack stale focus.
- **Input:** frozen subtrees are non-interactive; any input to a frozen item is a bug (assert in debug).
- **Timers/animations/network:** per §3.4 policies; `Frozen` implies `Timer.suspendWhen: Hidden` for timers *inside* the subtree unless a timer opts out with `suspendWhen: Never` (explicit beats implied).
- **Lifetime:** frozen ≠ destroyed. Memory stays; after N seconds in `Frozen` (default 30 s) and a pressure signal, allow **Dehydrate**: release GPU/derived data (textures, glyph runs, cached layouts) but keep QML objects; further pressure → **Unload** only for `Loader`-owned content, with `Ljos.Persist` state.
- **Testing:** property test — for any scripted event sequence, an observer subscribed to a model sees identical *final* values and identical *signal presence* (not count) with and without freezing; egui_kittest snapshot tests for thaw transitions.

### 3.6 Virtualized content (ListView/GridView/Repeater)

- Default `ListView` virtualizes with a pool (`reuseItems: true` default **on** in Ljós for delegates without `Component.onCompleted` side effects; Qt defaults off for compatibility — we have no legacy to protect). Emit `pooled`/`reused` signals compatible with Qt names so HAL-C2 QML ports 1:1.
- `cacheBuffer` default: **1 screen height each side on phone, 2 on desktop**; `displayMarginBeginning/End` supported. RN's `windowSize` of 21 is too generous for rich chat delegates; our default is ~3 screens total.
- **Time-sliced incubation:** create/bind at most **~2–4 ms** worth of delegates per frame (check with `Instant`), defer the rest to next frame; during flings prefer placeholders (blank height estimate) over frame drops (RN/Flutter accept blank flashes). Expose `Ljos.ListView.incubationBudgetMs` (default 3).
- Height estimation for variable rows: persist measured heights keyed by model id to keep scrollbar/anchor stable; anchor on *key + offset*, not index, for prepend (chat history loads).
- `Repeater` is *not* virtualized (QML semantic): document it; lint warning above N=50 items.
- `Loader { active; asynchronous }`: async means incubation on the same time-slice mechanism (no thread; QML objects are single-threaded in Ljós).

### 3.7 Memory policy and budgets

- **Images:** decode at `sourceSize` (or layout size × DPR when unset—stricter than Qt, where unset means full-size decode); decoded-bitmap LRU cache budget **min(64 MiB, 6 % of RAM) on phone, 256 MiB desktop**; async decode on a small thread pool (≤ 2 threads on phone); GPU textures counted in the same budget. Evict first: off-screen frozen pages' textures → unused cache entries → visible-adjacent.
- **Glyph atlas (parley/skrifa-generated glyphs into an egui font texture):** cap atlas at 2048² (A8 = 4 MiB) → grow to 4096² only on desktop; on pressure, reset atlas and re-rasterize on demand (cost is a one-frame hitch—acceptable at `UI_HIDDEN`).
- **Pressure mapping:** Android `TRIM_MEMORY_UI_HIDDEN` / winit `Suspended` → drop GPU caches + atlas; `onTrimMemory(BACKGROUND)` and `LowMemory` → also drop decoded image cache to 25 %, `run_gc()` on JS; iOS `memory_warning` → same as LowMemory. Define `Ljos.MemoryPressure {None, Low, Critical}` consumed by caches (a `trait Evictable { fn evict(&mut self, level) -> usize }`).
- **JS:** `set_memory_limit(96 MiB)` phone default, `set_gc_threshold(8 MiB)`; `run_gc()` on `Stop` and after bulk model teardown; expose `Ljos.debug.jsHeap` in a debug overlay. Interrupt handler enforces **JS time budget per event dispatch ≈ 4 ms soft (log) / 100 ms hard (abort with error)** to avoid ANR (Android ANR triggers at 5 s input dispatch, but jank starts at the frame budget).
- **Process budget targets (phone, typical chat screen):** RSS ≤ 150 MiB steady, GPU memory ≤ 64 MiB, cold start to first frame ≤ 500 ms on a mid-range device. Treat as initial budgets to be validated by benchmarks (§4).

### 3.8 Input, IME, gestures, scroll physics

- **IME:** on Android use GameTextInput through android-activity (`set_text_input_state`, `set_ime_editor_info` with `InputType` flags from QML `inputMethodHints`). Map composing region to Ljós's text-edit model (preedit with underline) and publish cursor/selection both ways. Report `keyboardRectangle` from `WindowInsets.ime` and animate with `WindowInsetsAnimation` callbacks (if available via the Java glue) else step.
- **Gestures:** one gesture arena in core: tap, double-tap, long-press (platform timeout via `ViewConfiguration.getLongPressTimeout`, default 400–500 ms), pan, pinch (reuse egui's `MultiTouchInfo` math or our own), with `touchSlop` (≈ 8 dp Android, 10 pt iOS) from the platform. QML-compatible handlers (`TapHandler`, `DragHandler`, `PinchHandler`, `MouseArea`) on top.
- **Scroll physics per platform as data:** `ScrollPhysics { friction, spline/exponential decel, overscroll: Stretch | Bounce | None, min_fling_velocity, max_fling_velocity }`; `VelocityTracker` (least-squares over last ~100 ms of samples). Android: spline fling + stretch overscroll (12+); iOS: exponential decel ~0.998/ms and rubber-band; desktop: wheel with smooth scrolling. Expose as `Flickable.physics` with platform default so HAL-C2 gets native feel without QML changes.
- **Back:** `Ljos.Back` router: handlers stack (topmost visible `StackView`/popup/drawer consumes). Support legacy mode and `OnBackInvokedCallback` mode; implement the latter in Java glue (a few dozen lines) when targetSdk moves to 36.
- **Safe area & orientation:** `SafeArea` = union of system bars, display cutout and (opt-in) ime insets, in dp; default window flags edge-to-edge; rotation re-layout in-process.
- **DPI:** scale factor from `Configuration.densityDpi / 160` combined with font scale (`fontScale` is separate: apply to text only, like Android). Never snap pixel scale to integers on mobile (2.625× is common).

### 3.9 Networking policy on mobile (tokio)

- One multi-thread tokio runtime with **2 worker threads on phone**, created in the app core (survives Activity). Use `rustls` + `rustls-platform-verifier` 0.7.1 (needs `jni` and `ndk-context`-style Android initialization) so system trust store and enterprise CAs work; `reqwest` 0.13.5 / `tokio-tungstenite` 0.30.0 are fine.
- **Foreground-only live connections:** subscribe to app lifecycle; on `Stop`/`UI_HIDDEN` + grace period (**~10–30 s**, for quick app switches) close WebSocket/SSE cleanly; on `Start` reconnect with a **resume cursor** (last seen event id) so UI catches up without re-fetching history. Rejected: keeping sockets alive via foreground service (battery, store policy, and iOS can't); relying on Android `Doze` exemptions.
- **Wake-ups:** FCM (Android) / APNs (iOS) data pushes handled by a tiny native component outside Ljós core; Ljós provides a `Push` plugin surface and a deep-link/notification-tap path into the router.
- Network change handling: `ConnectivityManager` callback → `Ljos.Network.online` property → exponential backoff with jitter (1 s → 30 s cap) reset on network change.
- Radio efficiency: batch non-urgent requests; avoid periodic polling <1/min in background (none in background anyway).

### 3.10 Build and packaging

- Android: Gradle project template + `cargo-ndk` 4.x producing `libljos_app.so` for arm64-v8a (+ x86_64 for emulators); GameActivity AAR; a `ljos` CLI (`ljos android run/bundle`) that wraps the steps. Minimum Android API 26 initially (setFrameRate needs 30; feature-detect `AChoreographer`/`ANativeWindow_setFrameRate`). Use AAB for Play.
- iOS: generated Xcode project (XcodeGen or similar) linking Rust static lib built per target (`aarch64-apple-ios`, `aarch64-apple-ios-sim`).
- **Rejected:** `cargo-apk` (NativeActivity-only, stale), `xbuild` (unmaintained), adopting `cargo-mobile2` wholesale (Tauri-shaped; we may borrow ideas). Reconsider `cargo-mobile2` only if its Android/iOS templates accept a custom Activity.

### 3.11 Frame-rate policy hints

`Window.frameRatePolicy`: `Auto` (default: high rate while animating/scrolling, drop to ~30–60 Hz or stop when static), `Max`, `Fixed(n)`. Implement via `ANativeWindow_setFrameRate(window, fps, ANATIVEWINDOW_FRAME_RATE_COMPATIBILITY_DEFAULT)` / `Surface.setFrameRate` on Android, `CAFrameRateRange` on iOS. Animations declare `frameRateHint` (`High` for scroll/gesture-driven, `Normal` for transitions, `Low` for spinners). Thermal: listen to `PowerManager.OnThermalStatusChangedListener` (API 29+) and cap to 60 Hz at `THERMAL_STATUS_MODERATE`+; reduce animation quality (skip blurs/shadows) at `SEVERE`.

### 3.12 Phased plan

1. **P0 (desktop, headless-testable):** scheduler, visibility lattice, lifecycle event model, `Evictable` caches, virtualized `ListView` with pool & time-slicing; kittest tests with fake clock.
2. **P1 (Android):** `AndroidHost` on android-activity+GameActivity, surface lifecycle, insets/IME, back, AccessKit `InjectingAdapter`, scroll physics; HAL-C2 mobile shell as acceptance app.
3. **P2:** freeze/dehydrate, UI-state persistence, predictive back, frame-rate/thermal policies, iOS host.

---

## 4. Risks / open questions

1. **Immediate-mode vs retained cost.** egui re-executes the UI each painted frame; per-frame cost scales with visible tree size. A retained display-list cache may be needed for long chat screens. *Measure before building:* benchmark 300 visible nodes with text on a mid-range Android device (target < 4 ms CPU/frame).
2. **GameActivity/GameTextInput maturity** for composing text (Icelandic dead keys, CJK, swipe/gesture typing, autofill, password managers). May require a Java-side `InputConnection` of our own. Needs an early IME spike.
3. **Predictive back** with progress animations is Java-side only; the glue must live in our Activity subclass. Test across OEM skins.
4. **wgpu on Android drivers:** Vulkan on older Mali/Adreno has had surface-loss and swapchain-recreate bugs; GL ES fallback quality unknown for egui-wgpu. Decide minimum GPU/API tier (Vulkan 1.1 on API 29+?).
5. **Freeze semantics:** coalescing change signals on thaw is a deliberate deviation from QML; need an opt-out and a lint pass for handlers with side effects. Real-world HAL-C2 QML (127 `HalC2.Bricks` files) should be scanned to see how many `on*Changed` handlers/timers would be affected.
6. **Animation logical-time catch-up** can reorder `finished` signals relative to model events; define ordering (run at thaw, in original deadline order).
7. **AccessKit on iOS (0.2.1)** is young; TalkBack/VoiceOver parity for text editing and scrolling lists (virtualized rows need `collection` semantics and stable ids).
8. **Store policy** for interpreted/downloaded QML/JS (Google Play Device and Network Abuse; Apple 2.5.2) — bundle-only is safe; plugin download is a legal/product question **[verify]**.
9. **Apple specifics unverified** (CADisableMinimumFrameDurationOnPhone, Metal-in-background, decel constants): confirm from Apple docs on a Mac.
10. **Budgets are initial guesses** (RSS 150 MiB, 3 ms incubation, caches 64 MiB, JS 96 MiB); validate with a benchmark app on low/mid/high devices, record in CI as perf-regression gates.
11. **Multi-window / foldables / split-screen / desktop mode on Android:** `VisibleUnfocused` state matters; `Resized` storms during fold; test with Jetpack WindowManager posture info (needs Java glue).
12. **Process death vs in-memory app core:** user-visible semantics when the whole process is killed (cold restore from snapshot + server resume cursor) need product decisions per screen (drafts, scroll position).

---

## 5. Sources

winit / android-activity / egui / wgpu / AccessKit / rquickjs
- https://docs.rs/winit/0.30.13/winit/platform/android/index.html
- https://docs.rs/winit/0.30.13/winit/platform/ios/index.html
- https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html
- https://docs.rs/winit/0.31.0-beta.3/winit/changelog/index.html
- https://docs.rs/winit/0.31.0-beta.3/winit/window/trait.Window.html
- https://raw.githubusercontent.com/rust-windowing/winit/v0.30.13/src/platform_impl/android/mod.rs
- https://raw.githubusercontent.com/rust-windowing/winit/v0.31.0-beta.3/winit-android/src/event_loop.rs
- https://docs.rs/android-activity/0.6.1/android_activity/
- https://docs.rs/android-activity/0.6.1/android_activity/enum.MainEvent.html
- https://docs.rs/android-activity/0.6.1/android_activity/struct.AndroidApp.html
- https://github.com/rust-mobile/android-activity
- https://github.com/emilk/egui/blob/main/CHANGELOG.md
- https://github.com/emilk/egui/issues/7748
- https://github.com/emilk/egui/issues/5590
- https://raw.githubusercontent.com/emilk/egui/main/crates/egui-winit/src/lib.rs
- https://raw.githubusercontent.com/emilk/egui/main/crates/eframe/Cargo.toml
- https://raw.githubusercontent.com/emilk/egui/main/crates/egui_kittest/README.md
- https://docs.rs/egui/0.36.2/egui/struct.Context.html
- https://wgpu.rs/doc/src/wgpu/documentation/platforms/android.rs.html
- https://docs.rs/wgpu/30.0.1/wgpu/enum.CurrentSurfaceTexture.html
- https://docs.rs/accesskit_android/0.9.0/accesskit_android/
- https://docs.rs/accesskit_android/0.9.0/accesskit_android/struct.InjectingAdapter.html
- https://docs.rs/crate/accesskit_winit/0.34.1/source/README.md
- https://github.com/AccessKit/accesskit
- https://docs.rs/rquickjs/0.14.0/rquickjs/struct.Runtime.html
- crates.io API (versions checked 2026-10): rustls-platform-verifier, tokio-tungstenite, reqwest, rxing, rqrr, jni, ndk-sys, ndk-context, accesskit_ios, cargo-mobile2

Build tooling and other toolkits' mobile docs
- https://github.com/tauri-apps/cargo-mobile2
- https://v2.tauri.app/develop/plugins/develop-mobile/
- https://dioxuslabs.com/learn/0.7/guides/platforms/mobile
- https://github.com/bevyengine/bevy/blob/main/examples/README.md
- https://github.com/bevyengine/bevy/blob/main/docs/cargo_features.md
- https://github.com/rust-mobile/xbuild
- https://github.com/rust-mobile/cargo-apk
- https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/android/
- https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/general/

Android / Apple platform
- https://developer.android.com/games/sdk/frame-pacing
- https://developer.android.com/media/optimize/performance/frame-rate
- https://developer.android.com/reference/android/content/ComponentCallbacks2
- https://developer.android.com/training/monitoring-device-state/doze-standby
- https://developer.android.com/develop/background-work/background-tasks
- https://developer.android.com/about/versions/16/behavior-changes-16
- https://developer.android.com/develop/ui/views/layout/edge-to-edge
- https://developer.android.com/guide/navigation/custom-back/predictive-back-gesture
- https://developer.android.com/games/agdk/game-activity
- https://developer.android.com/develop/ui/views/touch-and-input/gestures/scroll
- https://developer.android.com/develop/ui/compose/lifecycle
- https://developer.android.com/develop/ui/compose/graphics/draw/modifiers
- https://developer.apple.com/forums/thread/716118, /106838, /706740 (iOS socket suspension, DTS answers)
- https://support.google.com/googleplay/android-developer/answer/9888379 (Device and Network Abuse)

Qt (documentation only)
- https://doc.qt.io/qt-6/qml-qtquick-listview.html
- https://doc.qt.io/qt-6/qml-qtquick-loader.html
- https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html
- https://doc.qt.io/qt-6/qml-qtquick-controls-swipeview.html
- https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html
- https://doc.qt.io/qt-6/qml-qtquick-item.html
- https://doc.qt.io/qt-6/qml-qtquick-image.html
- https://doc.qt.io/qt-6/qml-qtquick-safearea.html
- https://doc.qt.io/qt-6/qml-qtquick-flickable.html
- https://doc.qt.io/qt-6/qt.html
- https://doc.qt.io/qt-6/qguiapplication.html
- https://doc.qt.io/qt-6/qquickitem.html
- https://doc.qt.io/qt-6/qtquick-performance.html

Prior art
- https://api.flutter.dev/flutter/widgets/TickerMode-class.html
- https://api.flutter.dev/flutter/widgets/Offstage-class.html
- https://api.flutter.dev/flutter/widgets/AutomaticKeepAlive-class.html
- https://docs.flutter.dev/perf/best-practices
- https://reactnative.dev/docs/optimizing-flatlist-configuration
- https://developer.chrome.com/blog/timer-throttling-in-chrome-88
- https://developer.mozilla.org/en-US/docs/Web/CSS/content-visibility

Local (read-only) reference: `/home/olafura/dev/hal-c2/apps/mobile-qt/` (README.md, CMakeLists.txt, android/AndroidManifest.xml, qml/HalC2/Mobile/*.qml, src/MobileApp.cpp) and `docs/internals/mobile-qt.md`.
