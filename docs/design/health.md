# Health

A window that freezes for a third of a second at every update, a handler that throws, a heap that fills until scripts stop: none of it shows on a still frame, and all of it is what a user meets first. The rules here judge how the application runs, rather than what it shows. Only the runtime audit checks them, from what the engine recorded while the application ran.

## How the audit sees it

- **Turns.** Each event (a timer, a message from a server, a key) runs as one turn of the UI thread, and nothing is drawn until the turn ends. The audit records every turn from the moment the host calls `ljos_audit::watch`, before the application loads, so its startup is judged too. An audit of a running application that was not watched (the inspector's `app_audit`) records only the turns of its own run, and stops recording when it is done. A turn's work is counted in the script interpreter's polls, one every ten thousand jumps and calls (a loop's turns, a function's calls), rather than timed: the same script does the same work on a laptop and on a loaded CI runner, so a finding is the same everywhere. `limits::health::TURN_WORK_MAX` polls is about 110 ms of plain script in an optimised build (`limits::health::POLL_MS` a poll).
- **Letting it run.** With a clock to run the application on (`Options::clock`) and leave to act, the audit lets the application run `limits::health::HEAP_WINDOWS` windows of `limits::health::HEAP_WINDOW_MS` of its own time, its timers and the host's updates included, collecting the garbage and reading the heap before and after each, and counting the frames of the last that showed something new while nothing animated. An application on the wall clock cannot be let run, and an audit without leave to act collects no garbage (that can run destruction handlers); the report's `notChecked` says which codes that left unjudged, and why.
- **What it does not see.** Work no script does (layout, drawing, image decoding) is no poll; `health-turn-slow` reads the wall clock for it, and is a hint because the wall clock is only as steady as the machine.

A host that feeds the application its data (a server's updates, a plugin's values) feeds it during those windows too, so the audit sees the application loaded rather than empty.

<a id="health-turn-over-budget"></a>
### `health-turn-over-budget`

**Error, runtime.** A turn ran more script than `limits::health::TURN_WORK_MAX` polls, and the window froze while it ran. The finding names the turn, its work, and the handler in it that did the most, with its file and line.

```qml
// Bad: every update rebuilds the whole list in one turn
onRowsChanged: { model.clear(); for (const r of rows) model.append(decorate(r)) }
// Good: apply what changed, and keep the heavy part out of the handler
onRowsChanged: model.sync(rows)   // a native model, or a diff
```

Split the work (a `Timer` per batch, `WorkerScript`), do less of it (apply the change, not the whole value), or move it out of script (a native model, a binding the engine evaluates once).

Sources: https://web.dev/articles/rail , https://lawsofux.com/doherty-threshold/ , https://doc.qt.io/qt-6/qtquick-performance.html

<a id="health-turn-slow"></a>
### `health-turn-slow`

**Hint, runtime.** A turn took longer than the turn budget (100 ms, or the engine's `LJOS_TURN_BUDGET_MS`) on this machine's clock. It also counts what no poll does (layout, drawing, native code); on a debug build script runs several times slower, which the finding says.

```qml
// Bad: a thousand delegates made at once
ListView { cacheBuffer: 100000 }
// Good: what is on screen, and a little more
ListView { cacheBuffer: 320 }
```

Sources: https://web.dev/articles/rail , https://doc.qt.io/qt-6/qtquick-performance.html

<a id="health-fault"></a>
### `health-fault`

**Error, runtime.** The application faulted while it ran: a handler threw, script ran out of memory, a native call panicked. One finding a kind of fault and place, with how often it came. A fault in a handler leaves whatever it was doing half done.

```qml
// Bad
Component.onCompleted: root.row.title = "Inbox"   // row is null until the data comes
// Good: a binding, which runs again when the row comes
Text { text: root.row ? root.row.title : "" }
```

Sources: https://doc.qt.io/qt-6/qtqml-javascript-expressions.html , https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Errors/Cant_access_property

<a id="health-loader-failed"></a>
### `health-loader-failed`

**Error, runtime.** A `Loader` is at `Loader.Error`, or is active with a source and made no item: what it should show is a hole. Loaders a delegate makes are reported once, as so many of the Loaders made at that place.

```qml
// Bad
Loader { source: "RowThatIsNotThere.qml" }
// Good
Loader { sourceComponent: rowComponent }
```

Sources: https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop

<a id="health-heap-headroom"></a>
### `health-heap-headroom`

**Error, runtime.** After a collection, the script heap holds so much of its cap that less than `limits::health::HEAP_HEADROOM_MIN` of it is free. A little more data and scripts fail with out of memory, which leaves handlers and bindings half run.

```qml
// Bad: every row's whole history held in script
property var history: rows.map(r => r.events)
// Good: what is shown, from a model that holds the rest natively
property var latest: rows.map(r => r.events[r.events.length - 1])
```

Sources: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Memory_management , https://doc.qt.io/qt-6/qtquick-performance.html

<a id="health-heap-growth"></a>
### `health-heap-growth`

**Error, runtime.** The script heap, after a collection, grew in every one of the windows the application ran, by at least `limits::health::HEAP_GROWTH_MIN_BYTES` in all: something keeps each update. Given time it fills the heap.

```qml
// Bad
onPublished: root.seen.push(value)
// Good
onPublished: root.latest = value
```

Sources: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Memory_management , https://developer.chrome.com/docs/devtools/memory-problems

<a id="health-never-settles"></a>
### `health-never-settles`

**Warning, runtime.** With nothing animating, the scene showed something new `limits::health::IDLE_FRAMES_MAX` times or more in a window, or never came to rest at all: something changes in every frame, and the window draws and the CPU stays awake while the user does nothing. A timer that fires and changes nothing shown is not counted. It is a warning, not an error: a ticker that changes every frame on purpose (a live waveform) can be what the design wants.

```qml
// Bad
Timer { interval: 16; repeat: true; running: true; onTriggered: dot.x += 1 }
// Good: an animation, which reduced motion and idle can stop
NumberAnimation on x { to: 190; duration: 3000; running: busy }
```

Sources: https://doc.qt.io/qt-6/qtquick-performance.html , https://web.dev/articles/rail

## Sources

- RAIL, the response and frame budgets: https://web.dev/articles/rail ; Laws of UX, Doherty threshold: https://lawsofux.com/doherty-threshold/
- Qt Quick performance: https://doc.qt.io/qt-6/qtquick-performance.html ; Loader status: https://doc.qt.io/qt-6/qml-qtquick-loader.html#status-prop
- Memory management in JavaScript: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Memory_management ; finding leaks: https://developer.chrome.com/docs/devtools/memory-problems
