---
title: Pattern matching with match and is
description: Say which of several cases a value is with match, test one pattern with is, and let the checker hold a match to every case of a union.
section: language
order: 8
---

# Pattern matching with `match` and `is`

After reading this page, you will be able to write a `match` that picks a value by the shape of another, test one shape with `is`, compare with a value in scope, write a chain of conditions, and read what the checker says when a `match` leaves a case out.

`match` and `is` are TC39's pattern-matching proposal, written as its README has it. Ljós adds no syntax of its own: a script is TypeScript, and a `match` is compiled away into plain TypeScript before anything else reads it.

## Where you can write it

A `match` goes in the scripts of a QML file of Ljós's own API (bindings, handlers and functions), and in an `.lts` module. A `.ts` module stays plain TypeScript, so that stock TypeScript tools read it as they read any other; a `match` in one is the error `ts-lts-syntax`, and `ljos migrate --write` renames the file to `.lts` and rewrites every import that names it, `qmldir` entries too:

```text
size.ts:3:10: error: `match` is Ljós syntax, which stock TypeScript tools cannot read in a `.ts` file; rename `size.ts` to `size.lts` [ts-lts-syntax]
```

An `.lts` module is imported as a `.ts` one is, and values and types cross both ways. A `name.ts` beside a `name.lts` is the error `module-clash` on both: one module name is one file.

## A value by its case

The arms are tried in order, and the first whose pattern matches gives the value. A literal matches the same value, with no coercion: `1` does not match `"1"`.

```qml
import Ljos.Ui

Window {
    width: 200
    height: 60
    visible: true

    property int unread: 3

    Text {
        text: match (unread) {
            when 0: qsTr("No messages");
            when 1: qsTr("One message");
            default: qsTr("%1 messages").arg(unread);
        }
    }
}
```

On a `number` or a `string`, a `match` needs a `default:`: there is always another value. On a union of literals, a plain enum or a union of records told apart by one key, leave `default` out, and the checker holds the `match` to every case. Adding a member to the union then makes every `match` that misses it an error, where a `default` would quietly take it.

```ts file=size.lts group=size
export type Size = "small" | "medium" | "large"

export function margin(size: Size): number {
    return match (size) {
        when "small": 4;
        when "medium": 8;
        when "large": 12;
    }
}
```

```qml group=size
import Ljos.Ui
import "size.lts" as Size

Window {
    width: 200
    height: 60
    visible: true

    Rectangle {
        x: Size.margin("large")
        width: 40
        height: 40
        fill: "steelblue"
    }
}
```

Leave out `when "large":`, and the check says which case is not handled, written as the pattern that would cover it:

```text
size.lts:4:12: error: `match` does not cover every case of `size`; for example, this case is not handled: `when "large":` [ts-match-exhaustive]
```

## Taking a record apart

An object pattern matches an object with those keys, each matching its own pattern, and `let` names what it matches. An array pattern matches an array of exactly that length; `...let rest` makes it "at least". `or` matches either side. A guard, `and if (condition)`, holds the arm to a condition on what it named.

This is the shape of a piece of work that is loading, done or failed:

```ts file=feed.lts group=feed
export type Feed =
    | { status: "pending" }
    | { status: "done"; titles: readonly string[] }
    | { status: "failed"; reason: string }

export function summary(feed: Feed): string {
    return match (feed) {
        when { status: "pending" }: "Loading…";
        when { status: "done", titles: [] }: "Nothing new";
        when { status: "done", titles: [let only] }: only;
        when { status: "done", let titles } and if (titles.length > 99): "99+ new";
        when { status: "done", let titles }: `${titles.length} new`;
        when { status: "failed", reason: "offline" }: "Offline";
        when { status: "failed" }: "Could not load";
    }
}

export const latest: Feed = { status: "done", titles: ["Ljós 1.0"] }
```

```qml group=feed
import Ljos.Ui
import "feed.lts" as Feed

Window {
    width: 200
    height: 60
    visible: true

    Text {
        text: Feed.summary(Feed.latest)
    }
}
```

There is no `default`, so the checker holds the `match` to every status. The guarded arm covers nothing for it, since the checker cannot know when `titles.length > 99`; the arm after it matches every `done` record whole, and the last arm covers `failed`.

## Comparing with a value in scope

A bare name in a pattern compares with the value it holds; it never binds. To bind, write `let`. A name that is not in scope is an error whose message gives the `let` fix.

```qml
import Ljos.Ui

Window {
    width: 200
    height: 60
    visible: true

    property string selected: "inbox"
    property string folder: "drafts/today"

    Rectangle {
        width: 40
        height: 40
        fill: match (folder) {
            when selected: "steelblue";
            when let name and if (name.startsWith("drafts/")): "gray";
            default: "black";
        }
    }
}
```

## A chain of conditions

A chain of conditions with nothing to take apart is a `match` whose arms are guards alone. Guards cover nothing, so the checker asks for a `default:`, which takes the place of a last `else`.

```qml
import Ljos.Ui

Window {
    width: 200
    height: 60
    visible: true

    property real distance: 40

    readonly property real clipNear: match (distance) {
        when if (distance < 1): 0.01;
        when if (distance < 100): 0.1;
        default: 1;
    }
}
```

## A look from several flags

A button picks one look from whether it is enabled, pressed and hovered. The `match` says that the looks exclude one another and which wins, in the order of its arms:

```qml
import Ljos.Ui

Window {
    width: 200
    height: 60
    visible: true

    Rectangle {
        width: 120
        height: 40
        fill: match (look) {
            when "pressed": "#1d4ed8";
            when "hovered": "#3b82f6";
            when "disabled": "#9ca3af";
            default: "#2563eb";
        }

        readonly property string look: match ({ enabled: enabled, down: tap.pressed, hovered: hover.hovered }) {
            when { enabled: false }: "disabled";
            when { down: true }: "pressed";
            when { hovered: true }: "hovered";
            default: "normal";
        }

        Tap {
            id: tap
        }

        Hover {
            id: hover
        }
    }
}
```

## A handler for each status

A `match` whose arms are calls stands as a statement, so it replaces a `switch` in a handler. With no `default`, a status added later is an error here rather than a case that falls through:

```ts file=access.lts group=access
export type Status = "granted" | "denied" | "undetermined"

export function next(status: Status): string {
    return match (status) {
        when "granted": "recenter";
        when "denied": "explain";
        when "undetermined": "ask";
    }
}
```

```qml group=access
import Ljos.Ui
import "access.lts" as Access

Rectangle {
    width: 200
    height: 60

    property string action: ""

    Tap {
        onTapped: action = Access.next("undetermined")
    }
}
```

## Testing one pattern with `is`

`x is pattern` is the same test as a boolean. Where it is `true`, `x` is narrowed, and what the pattern binds is in scope in the block around it:

```ts file=reply.lts group=reply
export type Reply = { status: number; body: string }

export function text(reply: Reply): string {
    if (reply is { status: 200, let body }) {
        return body
    }
    return `failed with ${reply.status}`
}
```

```qml group=reply
import Ljos.Ui
import "reply.lts" as Reply

Window {
    width: 200
    height: 60
    visible: true

    Text {
        text: Reply.text({ status: 200, body: "ok" })
    }
}
```

Reading `body` where the checker cannot prove the `is` was `true`, in the `else` or after the `if`, is the error `ts-is-binding`.

## What patterns there are

| Pattern | Matches | Example |
|---|---|---|
| literal | the same value; `NaN` matches `NaN`, and `0` matches `-0` too | `when "checked":`, `when 0:`, `when undefined:` |
| a name in scope | the same value, or, for a function, a `true` result of calling it with the subject | `when selected:`, `when isRow:` |
| primitive type | a value of that type | `when String:`, `when Number:` |
| regular expression | a string it finds a match in; no `g` or `y` flag | `` when /^https?:/: `` |
| binding | anything, and names it | `when let n:` |
| array | an array of that length, or at least, with `...` | `when [let x, let y]:`, `when [0, ...let rest]:` |
| object | an object with those keys | `when { status: 200, let body }:` |
| `and`, `or`, `not` | both, either, neither; mixing two needs parentheses | `when 301 or 304:`, `when not undefined:` |
| guard | anything, where the condition is `true` | `when let n and if (n > 0):` |

Extractors, custom matchers and the proposal's possible future additions, such as relational patterns, are not in it; a predicate or a guard says what they would.
