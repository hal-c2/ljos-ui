---
title: Tasks
description: Run async work from a Task's arguments, show where it got to through its state, stop it when its arguments change or its item goes, and have the checker type it from its run.
section: data
order: 8
---

# Tasks

After reading this page, you will be able to declare a `Task` that runs async work when its arguments change, show where it got to, run it when a button is pressed, and know what stops it.

## Run this when these change, and show where it got to

A `Task`, from `Ljos.Core`, holds one job: a `run` function, the `args` it is called with, and where the last run got to. When an element of `args` changes, the Task runs again. Its `state` says where the run is, and a binding that reads it follows it:

```qml
import Ljos.Core
import Ljos.Ui
import Ljos.Layout
import Ljos.Controls

Column {
    id: root
    spacing: 8
    padding: 12
    property string city: "Oslo"

    Task {
        id: weather
        args: [root.city]
        run: async (name: string, signal: AbortSignal): Promise<string> => {
            const reply = await fetch(`https://example.org/weather/${name}.txt`, { signal })
            if (!reply.ok) {
                throw new Error(reply.statusText)
            }
            return (await reply.text()).trim()
        }
    }

    function forecast(): string {
        const state = weather.state
        switch (state.status) {
            case "initial":
                return ""
            case "pending":
                return state.value ?? qsTr("Loading…")
            case "done":
                return state.value
            case "failed":
                return qsTr("No forecast")
        }
    }

    Label {
        text: root.forecast()
    }
    Button {
        text: "Bergen"
        onClicked: root.city = "Bergen"
    }
}
```

`run` takes the elements of `args`, then an `AbortSignal`. It may be async or plain: what it gives is the Task's value, and what it throws is its error. The Task starts its first run once it is complete.

`state` is one of four, told apart by `status`:

| `status` | also has | when |
|---|---|---|
| `"initial"` | | no run has started |
| `"pending"` | `value`, the last run's or `undefined` | a run is under way |
| `"done"` | `value` | the last run gave `value` |
| `"failed"` | `error`, and the last run's `value` | the last run threw, or was aborted |

A `switch` on `state.status` (or an `if` that compares it) narrows `state`, so in the `"done"` case `state.value` is a `string`, with no `undefined` to test for. A pending or failed state keeps the last run's value, so a page can keep showing the old forecast while the new one loads. `match` takes `state` apart the same way: see [Pattern matching](../language/match.md).

`status`, `value` (the value or `undefined`) and `error` are the same parts, read one at a time, for a binding that wants only one: `visible: weather.status === "pending"`. The four change together, and each notifies once a step, so a binding that reads two of them is evaluated once.

## A changed argument stops the run in progress

When `args` changes, element by element with `!==`, the Task aborts the run in progress through its signal and starts another. What a superseded run gives is dropped, even when it comes back after the new one starts, so a slow reply for `Oslo` never shows under `Bergen`. Give the signal to `fetch` and the superseded request stops; without it the request runs on, and only its result is dropped. `args` made again with the same elements runs nothing.

When the item the Task is declared in goes (a `Loader` unloads it, or a delegate is taken out of its view), the run in progress is aborted. A Task is a [Controller](../language/controllers.md) of that item, so a delegate scrolled out of a list stops its request.

A Task waiting on its run costs nothing: no timer and no frame. The reply wakes the application, which sets the Task's state and the bindings that read it, and sleeps again.

## Run when told

With `autoRun: false`, a Task runs only when `start()` is called: with the arguments it is given, or with `args`' own. `abort()` stops the run in progress, and the Task fails at once with the signal's reason, an `AbortError`. The signals `done(value)` and `failed(error)` are for what a binding cannot say, such as adding a row:

```qml
import Ljos.Core
import Ljos.Ui
import Ljos.Models
import Ljos.Layout
import Ljos.Controls

Column {
    id: root
    spacing: 8
    padding: 12

    ListModel {
        id: today
    }

    function activityOf(given: unknown): string {
        if (typeof given === "object" && given !== null && "activity" in given && typeof given.activity === "string") {
            return given.activity
        }
        throw new Error("the reply has no activity")
    }

    Task {
        id: randomActivity
        autoRun: false
        run: async (signal: AbortSignal): Promise<string> => {
            const reply = await fetch("https://example.org/api/activity", { signal })
            if (!reply.ok) {
                throw new Error(`status ${reply.status}`)
            }
            return root.activityOf(await reply.json())
        }
        onDone: (activity: string) => { today.append({ name: activity }) }
        onFailed: console.log("No random task: the service did not answer")
    }

    Button {
        text: randomActivity.status === "pending" ? qsTr("Asking…") : qsTr("Add a random task")
        onClicked: randomActivity.start()
    }
    Repeater {
        model: today
        Label {
            required property string name
            text: name
        }
    }
}
```

A reply that is not `ok` and a request that could not be made both fail the Task, and the page going aborts it. `error` is `unknown`, as a `catch` binding is: what a run throws need not be an `Error`, so narrow it before reading its `message`.

## What the checker holds a Task to

The checker types a Task from its `run`. `args`, written as an array, is held to the run's parameters before its signal, in number and element by element, as is what `start(…)` is given; `value` and `done` carry what the run's promise gives. A Task whose run takes a `name: string` and whose `args` is `[root.count]`, an `int`, is ``error: `name` is a string; this gives it a number [ts-assign-type]``, and reading `weather.value` into a `string` property is ``error: `last` is a string; this gives it a string or undefined [ts-assign-type]``. `ljos check --tsc` gives TypeScript the same Task.

Write the run's return type, as the examples do: a run that returns what a function of the file gives, with no type written, gives `unknown`.

## On the web

Apps built for the web do not have `Task` yet; it comes with `Controller` to ljos-web's runtime, with the same meaning.
