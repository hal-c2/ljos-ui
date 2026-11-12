---
title: TypeScript and JavaScript
description: Write typed TypeScript in bindings, handlers and modules, know what the checker holds it to, and understand how Ljós runs scripts in its register VM and QuickJS.
section: language
order: 6
---

# TypeScript and JavaScript

After reading this page, you will know what the checker holds TypeScript to, what JavaScript expressions and built-ins are available in QML, how imported `.js` files and `.pragma library` scripts work, how value types and QML objects behave in scripts, and which expressions run in the tier-1 register VM versus QuickJS.

## TypeScript first

In a file of Ljós's own API (one that imports `Ljos.Ui` and the other `Ljos.*` modules), handlers, functions and bindings are a strict subset of TypeScript. `ljos check`, the MCP `check` tool and hot reload check them in Rust before anything runs. Every program the checker accepts is valid TypeScript with the same meaning, so `tsc` and editors work on it as they do on any TypeScript. The checker closes the places where TypeScript lets a mistake through:

- no `==` or `!=` (`ts-loose-equality`), no `var` (`ts-no-var`) and no `null`: a value that may be missing is optional (`ts-no-null`);
- `if`, `while`, `?:` and `!` take a boolean, not a value's truthiness (`ts-condition-boolean`);
- no `any`, written or inferred, and no casts or `!`: a decoder narrows an `unknown` value instead (`ts-no-any`, `ts-no-cast`);
- a `switch` on a union covers every case (`ts-switch-exhaustive`), and indexing an array gives `T | undefined` (`ts-index-undefined`);
- a value goes only where its type does: a string goes to a `color`, and a union of literals takes only its members (`ts-assign-type`).

A `.ts` module imported with `import "filter.ts" as Logic` is held to the same rules, and every call the QML makes into it is checked against its types. A `.js` file a Ljós file imports is typed by a `.d.ts` file beside it. The types are erased when the file compiles, and what is left runs as the JavaScript described below.

Files of existing QML (those that import `QtQuick`) keep plain JavaScript and are not held to these rules.

## Expressions, functions, and string literals

Every binding in QML is a JavaScript expression. Operators follow JavaScript's exact coercion rules regardless of the types of the QML properties they read: `+` concatenates when either operand is a string and adds numbers otherwise, `-` and `*` convert strings to numbers, `==` converts before comparing while `===` does not, and `&&`, `||`, and `??` return one of their operands. Numbers convert to strings using the shortest round-tripping decimal representation.

You can declare JavaScript methods on any QML object with `function <name>(...) { ... }`. Method names must start with a lower-case letter and cannot use reserved global names such as `print`, `console`, `undefined`, `qsTr`, or `parseInt`. Calling a method on another QML object passes JavaScript values directly: `-0` keeps its sign, objects and arrays are passed by reference without copying, and exceptions thrown inside the callee can be caught by the caller with `try ... catch`.

```qml
pragma Scripts: JavaScript
import Ljos.Ui

Item {
    id: root
    property int count: 7
    property real ratio: 2.5
    property string label: "%1 items (%2px)".arg(count).arg(ratio * 4)

    function twice(v) {
        return v * 2
    }
}
```

In a `.qml` file, a single- or double-quoted string literal may span multiple lines; the line break (`\n`) and any leading spaces on the next line become part of the string. Imported `.js` files are parsed as standard JavaScript, where an unescaped line break inside a single- or double-quoted string is a syntax error.

## Imported `.js` files and `.pragma library`

You can move shared logic into a `.js` file and import it with `import "file.js" as Name`:

```javascript file=lib.js group=lib
// lib.js
.pragma library

var baseAddress = String(Qt.resolvedUrl("assets"))

function clamp(v, low, high) {
    return Math.max(low, Math.min(high, v))
}
```

```qml group=lib
// Main.qml
pragma Scripts: JavaScript
import Ljos.Ui
import "lib.js" as Lib

Item {
    property int value: Lib.clamp(140, 0, 100)
    property string assetsAt: Lib.baseAddress
}
```

A script file that begins with `.pragma library` is shared across all QML components that import it; a script without `.pragma library` is instantiated per importing component instance. In both cases:

- Relative paths passed to `Qt.resolvedUrl(...)` or `Qt.createComponent(...)` inside a `.js` file are resolved against the `.js` file's own path, regardless of which `.qml` file called the function.
- `Error` objects created or thrown in a binding, handler, or `.js` file carry `fileName` and `lineNumber` properties pointing to the file and line where the error was created.

## Value types and QML objects in JavaScript

When a JavaScript function reads a structured value type (`point`, `size`, `rect`, `color`, `font`) from a QML property into a local variable (`let`, `const`, or `var`) and mutates one of its fields, Ljós writes the modified value back to the source property:

```qml
import Ljos.Ui
import Ljos.Core

Item {
    id: root
    property point spot: point(1, 2)

    Text {
        id: label
        text: "Hello"
    }

    function moveAndStyle() {
        const p = spot
        p.x = 5
        const f = label.font
        f.bold = true
        f.pixelSize = 24
    }
}
```

If a local variable holding a value type is only read and never has a field mutated (or is passed as a function parameter and mutated inside the callee), it holds the value snapshot itself rather than a live reference to the property.

When serialising with `JSON.stringify`:

- A QML `QtObject` without parent/child cycles serialises its properties in declaration order (starting with `objectName`), omitting signals, methods, `undefined`, and functions. Colours, points, rects, and vectors serialise as objects of their fields, dates as ISO strings, URLs as strings, and lists as arrays.
- Calling `JSON.stringify` directly on an `Item` (or a `Timer`) throws `TypeError: Cannot convert circular structure to JSON` because `parent` and `children` refer back to the item, unless you pass a replacer function `JSON.stringify(item, (k, v) => ...)` that filters out circular keys.

## Built-in globals

In addition to the `Qt` global object and `qsTr`, Ljós provides standard JavaScript built-ins alongside QML-specific behaviours:

- **String and Array methods**: `String.prototype.arg` fills numbered placeholders (`"%1 of %2".arg(a).arg(b)`), alongside standard methods including `String.prototype.trimStart`, `trimEnd`, `replaceAll`, `at`, and `Array.prototype.flat` and `at`. Searching an empty string for `""` with `"".indexOf("")` returns `-1`, not the `0` that ECMAScript gives.
- **`Math.hypot`**: Divides its arguments by the largest magnitude and sums the squares in order.
- **`Date` and pinned wall clock**: `Date.parse` accepts ISO dates, standard written dates, and `Date`'s own output formats, returning `NaN` for single numbers or out-of-range days. In tests and replays, the host can pin the wall clock so `new Date()` and `Date.now()` advance deterministically with application time.
- **`URL`**: The `new URL(text, base)` global parses URLs and exposes `href`, `protocol`, `username`, `password`, `host`, `hostname`, `port`, `pathname`, `search`, `hash`, `origin`, and `searchParams` following the WHATWG URL standard (dropping default ports and returning `""` for missing parts).

## What runs where: Tier 1 VM and Tier 2 QuickJS

Ljós splits QML script execution across its register VM and its embedded QuickJS engine (`rquickjs`):

- **Tier 1 (register VM)**: Runs bindings composed of literals, statically resolved property reads and writes, value-type member reads, arithmetic, comparisons, logical operators, conditional (`? :`) expressions, template strings, `Math.*`, `qsTr`, `Qt.rgba`, `Qt.point`, `Qt.size`, `Qt.rect`, `String.prototype.arg`, and calls to user functions whose bodies also fit within Tier 1. Tier 1 evaluates the entire binding atomically without starting the JavaScript engine.
- **Tier 2 (QuickJS)**: Runs closures, loops, array and object literals, `JSON`, `RegExp`, `Date`, `URL`, imported `.js` modules, and imperative signal handlers. The QuickJS runtime is only initialised when a Tier 2 expression or `var` object/array property first requires it.

You can set `LJOS_TIER1=0` in the environment to force all bindings through Tier 2, or `LJOS_TIER1_CHECK=1` in release builds to cross-check every Tier 1 evaluation against Tier 2 (enabled by default in debug builds).

## See also

- [QML in Ljós](overview.md)
- [Properties and bindings](properties-and-bindings.md)
- [Signals and handlers](signals-and-handlers.md)
- [Modules and imports](modules-and-imports.md)
