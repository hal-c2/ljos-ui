# Spike 01: egui on Android: IME, AccessKit and lifecycle

Date: 2026-10-10. Milestone: M0 (input to M3: AndroidHost, the GameTextInput
bridge, accesskit_android, the lifecycle work). Code: a scratch eframe app and
Gradle project at `<scratch>/ljos-spike-android/` (outside the repo, not
committed).

## Questions

1. Does eframe/egui 0.36 run on Android with GameActivity, and what does that
   take?
2. IME: do the soft keyboard, typing, composition, backspace and cursor work?
   What does GameTextInput give, what do egui and winit drop, and what does the
   `TextInputClient` in ARCHITECTURE §11 need to fill the gaps?
3. Does AccessKit reach Android (uiautomator, `dumpsys accessibility`,
   TalkBack), with the right roles and names?
4. Lifecycle: home and relaunch, surface loss, back, rotation.
5. Versions, features, build commands, APK size and workarounds.

## Result

| Area | Result |
|---|---|
| eframe 0.36.2 + GameActivity on the emulator | **Works.** wgpu on Vulkan (SwiftShader). Cold start 254–287 ms (458 ms the first time), 0.0 % CPU when idle. |
| Soft keyboard show and hide | Works: `set_ime_allowed` maps to `show_soft_input`/`hide_soft_input`. |
| Typing from the soft keyboard into egui | **Does not work.** winit 0.30 drops android-activity's `TextEvent`, so egui never sees the text. GameTextInput holds it. |
| Composition, surrounding text, delete (GameTextInput) | Works at the GameTextInput level (checked with a scripted IME). Indices are **UTF-16 code units**. |
| Selection the IME sets, IME cursor updates | **Missing.** GameTextInput never calls `InputMethodManager.updateSelection`; an IME `setSelection` does not reach `text_input_state()`. |
| `set_ime_cursor_area`, `set_ime_purpose` | No-ops in winit 0.30. |
| Editor info (input type, caps, password, number) | Works through `set_ime_editor_info`. The action argument does not reach `imeOptions` (see below). |
| AccessKit to Android | **Works once the feature is turned on by hand.** Roles, names and bounds are right; TalkBack focuses nodes and a double-tap clicks them. |
| AccessKit stability | **Crash** in accesskit_android 0.7.5 once accessibility is turned off. Fixed in 0.9.0, which needs accesskit 0.25 (egui 0.36 is on 0.24). |
| HOME and relaunch | Works (hot relaunch 31 ms, surface dropped and recreated). |
| BACK | **Deadlock**, then ANR on relaunch. Worked around with a back callback in Java. |
| Rotation | **ANR** unless `configChanges` is set. With `configChanges` it works. |
| Window insets (keyboard, system bars) | **Not available.** The keyboard covers the UI and the content rect does not change. |

### Stack used

- eframe, egui, egui-winit, egui-wgpu **0.36.2**. eframe features:
  `accesskit`, `default_fonts`, `wgpu`, `android-game-activity`
  (`default-features = false`).
- winit **0.30.13** (`android-game-activity`), android-activity **0.6.1**
  (game-activity backend), ndk 0.9.0, jni 0.21.1/0.22.4,
  android_logger 0.15.1.
- wgpu 30.0.1.
- accesskit 0.24.1, accesskit_winit 0.32.2 **with `accesskit_android`**,
  accesskit_android 0.7.5 (`embedded-dex`).
- Java side: `androidx.games:games-activity:4.4.0` (the version android-activity
  0.6.1 is written against; no prefab), appcompat 1.7.1, core 1.17.0.
- AGP 8.13.2, Gradle 8.13 on JDK 21, compileSdk/targetSdk 36, minSdk 26,
  NDK r30 (30.0.16248370), Rust 1.98.1, `cargo ndk -P 26`.
- Emulator: AVD `ljos36`, android-36 google_apis x86_64, 1080x2400
  at 420 dpi (ppp 2.625), Gboard, TalkBack 16.0.0.738667889.

Newest on crates.io on 2026-10-10: accesskit_android 0.9.0 (2026-09-25),
accesskit_winit 0.34.1, egui-winit 0.36.2, android-activity 0.6.1, winit
0.31.0-beta.3.

### APK size (release, no R8, debug-signed)

| ABI | APK | `libljos_spike_app.so` (stored) | Notes |
|---|---|---|---|
| x86_64 | 16,408,457 B | 13,582,472 B | |
| arm64-v8a | 14,773,988 B | 11,948,000 B | 17,383,816 B before stripping |

`classes.dex` is 6,574,456 B (2.37 MB deflated), most of it appcompat; R8 and a
lighter base Activity would cut that. The `.so` needs only `liblog`, `libdl`,
`libandroid`, `libm` and `libc`.

## Findings

### 1. AccessKit is off on Android unless you turn it on

egui-winit enables `accesskit_winit` without its `accesskit_android` feature, so
on Android the adapter silently falls back to the null platform adapter and
nothing reaches the accessibility tree. The fix is a direct dependency that
turns the feature on:

```toml
accesskit_winit = { version = "0.32.2", features = ["accesskit_android"] }
```

### 2. Typed text never reaches egui

With GameActivity the soft keyboard talks to GameTextInput, which owns the
editable text and sends android-activity `InputEvent::TextEvent` (and
`TextAction`). winit 0.30 logs these as "Unknown android_activity input event"
and drops them. egui gets only stray key-ups (Backspace, A, B, C...) and, on an
empty editor, a full DEL press. Typing "hi" in an egui `TextEdit` gives nothing
in the field, while `text_input_state()` holds "hi".

winit 0.31.0-beta.3 (the new `winit-android` crate) does not fix this either:
it has no `TextEvent`, `TextAction` or `set_text_input_state` handling;
`request_ime_update` handles only `Enable` (show keyboard), `Disable` (hide)
and an `Update` that does nothing; `set_ime_cursor_area` and `set_ime_purpose`
are empty.

### 3. What GameTextInput does give (scripted IME)

A small InputMethodService in the spike (`ScriptIme`, driven by
`am broadcast`) called InputConnection methods directly, while the app logged
`text_input_state()` every change:

| IME call | `text_input_state()` after |
|---|---|
| `setComposingText("k")` | text "k", sel (1,1), compose (0,1) |
| `setComposingText("か")`, then `"かな"` | compose (0,1), then (0,2) |
| `commitText("仮名")` | compose cleared |
| `setComposingText("x")` | compose (2,3) |
| `finishComposingText()` | compose cleared |
| `setComposingRegion(0,2)` | compose (0,2) |
| `deleteSurroundingText`, `deleteSurroundingTextInCodePoints` | applied correctly |
| `getTextBeforeCursor`, `getTextAfterCursor`, `getSelectedText` | correct answers |
| `sendKeyEvent(KEYCODE_DEL)` | reaches egui as Backspace press and release |
| `performEditorAction(6)` (Done) | reaches nothing in egui |
| `setSelection(0,2)` | Editable changes (`getSelectedText` = "ab"), **`text_input_state()` does not** |

So composition, surrounding text and deletion live correctly in GameTextInput.
The holes are in getting them to the app and back to the IME.

Gboard on the emulator commits one character at a time (`commitText`) and does
not compose, both in GameTextInput and in a stock `EditText` (control
activity). That is Gboard's behaviour with this setup, not a GameTextInput
defect, so composition had to be checked with the scripted IME.

### 4. Gaps the `TextInputClient` (ARCHITECTURE §11) must fill

1. **Text events.** Ljós must consume android-activity's `TextEvent` itself
   and turn each new `TextInputState` into `replace`/`set_selection` calls on
   the focused client (a diff against the last state, since GameTextInput sends
   whole states).
2. **UTF-16 indices.** Selection and compose ranges are UTF-16 code units, and
   android-activity passes them through unchanged while the text is a Rust
   UTF-8 `String`. For "仮名x😀þ" (13 bytes, 5 chars, 6 UTF-16 units) the
   cursor at the end is 6. The app seeding "þú😀" with selection 8 (bytes)
   produced sel (0,0) silently, because 8 is out of range for 4 UTF-16 units.
   The bridge must convert both ways and must test non-BMP characters.
3. **Selection back to the IME.** GameTextInput never calls
   `InputMethodManager.updateSelection`, so the IME does not learn about cursor
   moves made by the app (tap, arrow keys, `set_selection`). Gboard's
   suggestions and the shift state go stale. The host needs Java/Kotlin glue
   that calls `updateSelection` after each `set_text_input_state`.
4. **IME-set selection.** An IME `setSelection` is not surfaced to native code;
   same glue (or a patched GameTextInput) needed.
5. **Initial state.** `EditorInfo.initialSelStart` is -1 for a fresh or empty
   state, and also when text exists but no state was set yet. It is 0,0 only
   after a `set_text_input_state`. The initial compose region reads
   `Some((0,0))` rather than `None`. Set the state before showing the keyboard.
6. **Editor action.** `set_ime_editor_info(input_type, action, options)`:
   input type works (CAP_SENTENCES gives 0xc001, password 0x81, number 0x2) and
   reaches the IME, but the `TextInputAction` argument does not reach
   `imeOptions` (it stays 0x2000000); android-activity passes it as the action
   id. Put the action in the action bits of `ImeOptions` instead. Pressing the
   action key (`performEditorAction`) is not delivered to egui at all; it comes
   as `TextAction`, which winit drops.
7. **Cursor rect.** No `CursorAnchorInfo`: GameTextInput logs "Request cursor
   updates: 2" and does nothing. `cursor_rect` in §11 needs the Kotlin
   fallback or new glue to matter on Android.
8. **Input purpose and hints** do not go through winit; use
   `set_ime_editor_info` directly.
9. **Insets.** See lifecycle below: without insets the focused field can sit
   under the keyboard.
10. **Fonts.** egui's default fonts have no CJK glyphs ("名" draws as tofu).
    Android needs system font fallback (`/system/fonts`, `fonts.xml`).

### 5. AccessKit on Android

uiautomator (`uiautomator dump`) shows the egui tree with correct classes,
names and bounds in physical pixels:

| egui widget | Android class | Name |
|---|---|---|
| `Label` | `TextView` | text = label |
| `TextEdit` (both) | `EditText` | text empty, hint empty |
| `Button` | `Button` | `content-desc` = label |
| `Checkbox` | `CheckBox` | checkable, `content-desc` = label |

Gaps: `clickable=false` on Button and CheckBox; the `TextEdit` hint ("type
here") is not exposed; the `EditText` text is empty.

TalkBack 16: touch exploration focuses the AccessKit nodes (green focus ring on
Send and on the checkbox) and a double-tap produces an AccessKit `Click`
action request, which egui handles (Send clicked, checkbox toggled). TalkBack on
costs about 0.5 ms of `ui()` time per frame for 300 labels (see spike 03).

**Crash in accesskit_android 0.7.5.** `event.rs:64` unwraps the result of
`requestSendAccessibilityEvent`, which returns a JavaException once
accessibility is off. Reproduced two ways: (a) after a `uiautomator dump`
ends, the next focus change (tap on a field) aborts the process; (b) turning
TalkBack off while frames are updating aborts it. The log shows "failed to
initiate panic, error 5" (the panic crosses JNI). accesskit_android 0.9.0 fixes
this ("Don't raise events if Android accessibility gets disabled", #803; also a
detached-host fix, #800), but 0.9.0 needs accesskit 0.25 and egui 0.36 is on
0.24. This matches spike 04's note about the 0.24/0.25 split.

### 6. Lifecycle

| Action | Result |
|---|---|
| Cold start | 254–287 ms to first frame (458 ms on the very first run). |
| HOME, relaunch | Surface dropped ("No window - clearing all surfaces"), hot relaunch 31 ms, state kept. |
| BACK | Activity finishes; `GameActivity.onDestroy` blocks the UI thread in `pthread_cond_wait` until `android_main` returns, but `android_main` sits in `ALooper_pollOnce` because winit 0.30 ignores `MainEvent::Destroy`. "Activity destroy timeout", and the relaunch hangs on the splash screen (ANR). |
| Rotation, no `configChanges` | Activity recreated, black screen, then ANR (same deadlock). |
| Rotation, with `configChanges` | Works: content rect goes to 914.3x411.4 pt and back. |
| Keyboard shown | Content rect does not change; the keyboard covers the bottom of the UI. `adjustResize` does nothing edge-to-edge (targetSdk 36). |

android-activity 0.6.1 exposes no insets: `MainEvent::InsetsChanged {}` has no
data, its FFI module is private, and winit 0.30 drops the event anyway (0.31
logs "TODO: handle Android InsetsChanged notification"). The UI draws
edge-to-edge under the status and navigation bars.

Workarounds checked in the spike:

1. **`configChanges`** on the activity:
   `orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden|navigation|density|uiMode|fontScale|locale|layoutDirection`.
2. **Back:** `onBackPressed()` is never called on targetSdk 36 (predictive
   back), so override it does nothing. An `OnBackPressedDispatcher` callback
   that calls `moveTaskToBack(true)` works: back then relaunch is hot (20 ms).
   egui also gets a `BrowserBack` key.
3. `gradle.properties`: `org.gradle.java.home` set to JDK 21, because Gradle
   8.13 cannot run on the system JDK 26.

## Reproduce

Project layout under `<scratch>/ljos-spike-android/`:
`src/` is a cargo workspace (`app` = `ljos_spike_app`, `bench` =
`ljos_spike_bench`) plus `src/android/` (Gradle) and helper scripts; `shots/`
has screenshots 01–23; `stack-after-back.txt`, `crash1.log`, `ui1.xml`,
`ime-dump.txt` are the raw evidence.

```
# Build (sets TMPDIR and GRADLE_USER_HOME under <scratch>, ANDROID_HOME, NDK r30)
src/build-apk.sh x86_64          # or arm64-v8a
#   = cargo ndk -t x86_64 -P 26 -o android/app/src/main/jniLibs build -p ljos_spike_app --release
#   + gradle --no-daemon -q assembleRelease   (Gradle 8.13, JDK 21)
adb install -r src/android/app/build/outputs/apk/release/app-release.apk
adb shell am start -n is.ljos.spike/.MainActivity
adb logcat -s SPIKE-EGUI SPIKE-IME SPIKE-LIFE SPIKE-SIME SPIKE-REF
```

Scripted IME (composition without a real keyboard):

```
adb shell ime enable is.ljos.spike/.ScriptIme && adb shell ime set is.ljos.spike/.ScriptIme
adb shell am broadcast -p is.ljos.spike -a is.ljos.spike.IME --es op compose --es text か
#   integer arguments: --ei a N --ei b M (region, delete, select, action); --ei code K (key)
#   ops: compose, commit, finish, region, delete, deletecp, select, key, action, info
adb shell ime reset   # back to Gboard
```

Force-stopping the app also kills the IME process (same package); run
`ime set` again afterwards.

TalkBack: `adb shell settings put secure enabled_accessibility_services
com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService`
and `accessibility_enabled 1`. `adb shell input tap` bypasses touch
exploration; `src/rawtap.sh X Y` (needs `adb root`) sends a real multi-touch
sequence with pressure through `sendevent` to `/dev/input/event2`, which
TalkBack does see. Run it twice quickly for a double-tap. The first TalkBack
start asks for a notification permission; grant
`android.permission.POST_NOTIFICATIONS` to `com.google.android.marvin.talkback`.

The 50 ms `text_input_state()` poller in the spike app is a diagnostic only;
it breaks the 0 % idle rule and must not be copied.

## Recommendations for M3

1. **Do not go through winit's Android backend for input.** Neither 0.30 nor
   0.31 delivers `TextEvent`/`TextAction`, handles `Destroy` in 0.30, or
   forwards insets. `ljos_shell`'s AndroidHost should drive android-activity
   directly (its own loop, `InputEvent::TextEvent` into `TextInputClient`,
   `MainEvent::Destroy` returning from `android_main`), and feed egui
   `RawInput` itself. Patching winit 0.30 is the fallback; waiting for 0.31
   does not solve IME.
2. **Own the accessibility adapter.** Use accesskit_android 0.9 (needs
   accesskit 0.25) through the AccessKit graft from spike 04, not egui's
   built-in 0.24 path, to avoid the 0.7.5 crash. Fill `clickable`, the hint and
   the edit text.
3. **Ship a small Java/Kotlin layer in the Gradle template** (ARCHITECTURE
   already plans a Kotlin InputConnection fallback):
   - `updateSelection` after each state change, and report IME-set selection;
   - `CursorAnchorInfo` for `cursor_rect`;
   - an insets listener (`WindowInsetsCompat`: system bars, IME, cutout) sent
     to native code for SafeArea and keyboard avoidance;
   - an `OnBackPressedDispatcher` callback for the back router (predictive back),
     never `finish()` while the native loop can block `onDestroy`;
   - `configChanges` in the template manifest; handle the config change in
     Rust.
4. **UTF-16 conversion** in the GameTextInput bridge, with tests for non-BMP
   text, and set the state before showing the keyboard.
5. **Put the editor action in `ImeOptions`** and handle `TextAction`.
6. **System font fallback** for CJK and other scripts.
7. Test on a real device with a composing IME (Gboard with Japanese or Korean,
   or a hardware keyboard) since the emulator's Gboard never composed.
8. Turn on R8 and drop appcompat if possible; check APK size again.
