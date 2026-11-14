# Sources:
#   docs/DIALECT.md (§4.18 `match` and `is`, §4.21 the desugar, §5.1 `ts-match-exhaustive` and `ts-is-binding`)
#   https://github.com/tc39/proposal-pattern-matching at commit ba73ffe42f30f0cf56ece28bebdc28f955f2a52e
#     (2025-10-03): its README and spec.emu (InvokeCustomMatcher, ListPatternMatches, FinishListMatch,
#     Function.prototype[%Symbol.customMatcher%], String[%Symbol.customMatcher%])
# `match` and `is` are TC39's pattern matching, which Qt's QML does not have: there is no oracle.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Choosing a value with match, and testing one with is
  A script says "which of these is it?" with `match`: its arms are tried
  in order, and the first whose pattern matches the subject gives the
  value. A pattern is a literal, a name to compare with or call, `String`,
  `Number` or `Boolean`, a regular expression, a binding (`let x`), an
  array, an object, an extractor (`Option.Some(let x)`), or `and`, `or`
  and `not` of those, and `if (c)` is a pattern that holds where `c` does.
  An object with a `Symbol.customMatcher` decides for itself what it
  matches, and what an extractor takes apart. `x is pattern` is the same test as a
  boolean. In a file written in Ljós's API, a `match` with no `default`
  must cover every case of its subject, and the checker shows a case it
  leaves out.

  Rule: The first arm that matches gives the value

    Scenario: Literals, either of two, conditions and a default
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int minutes: 75
          readonly property string span: match (minutes) {
            when if (minutes < 60): "minutes";
            when if (minutes < 1440): "hours";
            default: "days";
          }
          property int code: 304
          readonly property string kind: match (code) {
            when 200: "ok";
            when 301 or 304: "moved";
            when let c and if (c >= 500 && c < 600): "failed " + c;
            default: "other";
          }
        }
        """
      Then the property "span" of "root" is "hours"
      And the property "kind" of "root" is "moved"
      When the property "minutes" of "root" is set to 3000
      And the property "code" of "root" is set to 503
      Then the property "span" of "root" is "days"
      And the property "kind" of "root" is "failed 503"

    Scenario: NaN matches NaN, 0 matches -0, and -0 only itself
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real a: NaN
          property real b: -0
          property real c: 0
          readonly property string first: match (a) { when NaN: "nan"; default: "number"; }
          readonly property string second: match (b) { when 0: "zero"; default: "other"; }
          readonly property string third: match (c) { when -0: "minus zero"; when 0: "zero"; default: "other"; }
        }
        """
      Then the property "first" of "root" is "nan"
      And the property "second" of "root" is "zero"
      And the property "third" of "root" is "zero"

    Scenario: Objects and arrays name their parts, and a guard has the last word
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var reply: ({ status: 200, body: "" })
          readonly property string said: match (reply) {
            when { status: 200, let body } and if (body !== ""): body;
            when { status: 200 }: "empty";
            when { status: let s }: "status " + s;
            default: "none";
          }
          property var pair: [3, 4, 5]
          readonly property real sum: match (pair) {
            when [let x, let y]: x + y;
            when [let x, ...let rest]: x + rest.length;
            default: -1;
          }
          readonly property bool missing: match (reply) { when { error }: true; default: false; }
          readonly property string nested: match ({ at: [1, { name: "n" }] }) {
            when { at: [1, { name: let n }] }: n;
            default: "";
          }
        }
        """
      Then the property "said" of "root" is "empty"
      And the property "sum" of "root" is 5
      And the property "missing" of "root" is false
      And the property "nested" of "root" is "n"
      When the property "reply" of "root" is set to {"status": 200, "body": "hi"}
      And the property "pair" of "root" is set to [1, 2]
      Then the property "said" of "root" is "hi"
      And the property "sum" of "root" is 3

    Scenario: not, and patterns grouped in parentheses
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var value: 7
          readonly property string what: match (value) {
            when undefined: "nothing";
            when not (0 or 1): "many";
            default: "few";
          }
        }
        """
      Then the property "what" of "root" is "many"
      When the property "value" of "root" is set to 1
      Then the property "what" of "root" is "few"

    Scenario: A name in a pattern compares with its value, or calls it when it is a function
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string selected: "b"
          property var row: "b"
          property int n: 4
          function isEven(v) { return v % 2 === 0 }
          readonly property string look: match (row) { when selected: "highlight"; default: "plain"; }
          readonly property string parity: match (n) { when isEven: "even"; default: "odd"; }
        }
        """
      Then the property "look" of "root" is "highlight"
      And the property "parity" of "root" is "even"
      When the property "row" of "root" is set to "a"
      And the property "n" of "root" is set to 3
      Then the property "look" of "root" is "plain"
      And the property "parity" of "root" is "odd"

    Scenario: String, Number and Boolean test the type, and a regular expression the text
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var value: "https://example.org"
          readonly property string type: match (value) {
            when String: "text";
            when Number: "number";
            when Boolean: "flag";
            default: "other";
          }
          readonly property string where: match (value) {
            when /^https?:/: "web";
            when /^file:/i: "disk";
            default: "elsewhere";
          }
        }
        """
      Then the property "type" of "root" is "text"
      And the property "where" of "root" is "web"
      When the property "value" of "root" is set to 3
      Then the property "type" of "root" is "number"
      And the property "where" of "root" is "elsewhere"

    Scenario: is tests a value as a pattern
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var reply: ({ status: 204 })
          readonly property bool ok: reply is { status: 200 or 204 }
          readonly property bool listed: reply is []
          readonly property bool both: reply.status is 204 && !(reply is { error })
        }
        """
      Then the property "ok" of "root" is true
      And the property "listed" of "root" is false
      And the property "both" of "root" is true

    Scenario: What is binds is seen in the block around it
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var reply: ({ status: 200, body: "hi" })
          property string said: ""
          function read() {
            if (reply is { status: 200, let body }) {
              said = body
            }
            if (!(reply is { let status })) return
            said = said + " " + status
          }
          Component.onCompleted: read()
        }
        """
      Then the property "said" of "root" is "hi 200"

    Scenario: A match and an is may await, in the subject and in an arm, and read it in order
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          property var size: async function (key) {
            const log = []
            const n = match (await Promise.resolve(key)) {
              when "s": 4;
              when "m": (log.push("arm"), await Promise.resolve(8));
              default: 0;
            }
            log.push("after")
            const ok = (await Promise.resolve({ status: 200 })) is { status: 200 }
            return `${n} ${ok} ${log.join(",")}`
          }
          Component.onCompleted: size("m").then(v => said = v)
        }
        """
      When 10 milliseconds pass
      Then the property "said" of "root" is "8 true arm,after"

    Scenario: A match and an is in a generator yield from the subject and an arm, and the match is the arm's value
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          property var steps: function* () {
            const n = match (yield 1) {
              when "a": (yield arguments[0] + this.k) + 1;
              default: 0;
            }
            const ok = (yield n) is "b"
            return `${n} ${ok}`
          }
          Component.onCompleted: {
            const run = steps.call({ k: 3 }, 10)
            const seen = [run.next().value, run.next("a").value, run.next(5).value]
            const last = run.next("b")
            said = `${seen.join(" ")} | ${last.value} ${last.done}`
          }
        }
        """
      Then the property "said" of "root" is "1 13 6 | 6 true true"

    Scenario: A match and an is in an async generator await and yield, and the match is the arm's value
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          property var steps: async function* () {
            const n = match (yield await Promise.resolve(1)) {
              when "a": (yield arguments[0] + this.k) + await Promise.resolve(1);
              default: 0;
            }
            const ok = (yield await Promise.resolve(n)) is "b"
            return `${n} ${ok}`
          }
          Component.onCompleted: (async () => {
            const run = steps.call({ k: 3 }, 10)
            const seen = [(await run.next()).value, (await run.next("a")).value, (await run.next(5)).value]
            const last = await run.next("b")
            said = `${seen.join(" ")} | ${last.value} ${last.done}`
          })()
        }
        """
      When 10 milliseconds pass
      Then the property "said" of "root" is "1 13 6 | 6 true true"

    Scenario: Two is in one block bind one name, and an inner block's is shadows an outer let
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var a: ({ body: "a" })
          property var b: ({ body: "b" })
          property string said: ""
          function read() {
            let body = "outer"
            {
              if (a is { let body }) said += body
              if (b is { let body }) said += body
            }
            said += " " + body
          }
          Component.onCompleted: read()
        }
        """
      Then the property "said" of "root" is "ab outer"

    Scenario: What an is in a switch's clause binds is declared before the switch, so every clause may be entered
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          function pick(v: number, a: number, b: number): number {
            switch (v) {
              case 1: if (a is let x) return x; break
              case 2: if (b is let x) return x
            }
            return 0
          }
          Component.onCompleted: said = pick(2, 0, 7) + " " + pick(1, 5, 0)
        }
        """
      Then the property "said" of "root" is "7 5"

    Scenario: A match whose arms are calls is a statement, after a line with no semicolon
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string mode: "b"
          property int count: 0
          function step() {
            count = 1
            match (mode) {
              when "a": count += 10;
              when "b": count += 20;
              default: count += 30;
            }
          }
          Component.onCompleted: step()
        }
        """
      Then the property "count" of "root" is 21

    Scenario: A match that nothing matches throws a TypeError
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string mode: "c"
          property string said: "unset"
          Component.onCompleted: {
            try {
              said = match (mode) { when "a": "a"; when "b": "b"; }
            } catch (e) {
              said = e instanceof TypeError ? "type error" : "other";
            }
          }
        }
        """
      Then the property "said" of "root" is "type error"

  Rule: An object's custom matcher decides what it matches, and an extractor takes apart the list it gives

    Scenario: A custom matcher is asked with the hint "boolean", a predicate is given the subject alone
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            const hints = [];
            const Even = { [Symbol.customMatcher](s, hint) { hints.push(hint); return s % 2 === 0 ? 1 : 0; } };
            const isSmall = (s, strict = "own") => { hints.push(strict); return s < 3; };
            const a = match (4) { when Even: "even"; default: "odd"; };
            const b = 3 is Even;
            const c = match (2) { when isSmall: "small"; default: "big"; };
            said = a + " " + b + " " + c + " " + hints.join(",");
          }
        }
        """
      Then the property "said" of "root" is "even false small boolean,boolean,own"

    Scenario: An extractor matches the list its matcher gives with the hint "list", item by item
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            const hints = [];
            const Point = { [Symbol.customMatcher](s, hint) { hints.push(hint);
              return typeof s === "object" && s !== null && "x" in s ? [s.x, s.y] : false; } };
            const Option = { Some: { [Symbol.customMatcher](s) { return s.tag === "some" ? [s.value] : false; } } };
            const pair = (s) => Array.isArray(s) && s.length === 2 ? s : false;
            const show = (v) => match (v) {
              when Point(let only): "one " + only;
              when Point(let x, 0): "on the x axis at " + x;
              when Point(let x, let y): "at " + x + "," + y;
              when Option.Some(String(let s)): "some text " + s;
              when Option.Some(let n): "some " + n;
              when pair(1, ...let rest): "pair from 1 to " + rest[0];
              when pair(...): "a pair";
              default: "nothing";
            };
            said = [show({ x: 2, y: 0 }), show({ x: 1, y: 5 }), show({ tag: "some", value: "a" }),
                    show({ tag: "some", value: 7 }), show([1, 9]), show([2, 9]), show({ tag: "none" })].join("; ")
                 + " / " + hints[0];
          }
        }
        """
      Then the property "said" of "root" is "on the x axis at 2; at 1,5; some text a; some 7; pair from 1 to 9; a pair; nothing / list"

    Scenario: is takes apart what a function gives, and binds what it names in the block around it
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var reply: ({ tag: "some", value: 42 })
          function some(s) { return s.tag === "some" ? [s.value] : false; }
          readonly property bool number: reply is some(Number)
          readonly property bool text: reply is some(String) or { tag: "none" }
          property string said: ""
          Component.onCompleted: {
            const r = reply;
            if (r is some(let n) and if (n > 40)) said = "big " + n;
          }
        }
        """
      Then the property "number" of "root" is true
      And the property "text" of "root" is false
      And the property "said" of "root" is "big 42"

    Scenario: An extractor reads its list one item past its patterns, or only as far as a rest needs
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            let reads = 0;
            const Count = { [Symbol.customMatcher](s) {
              return (function* () { for (let i = 0; i < 100; i++) { reads++; yield i; } })();
            } };
            const exact = match (0) { when Count(0, 1): "two"; default: "more"; };
            const r1 = reads;
            reads = 0;
            const least = match (0) { when Count(0, ...): "at least one"; default: "none"; };
            said = exact + " " + r1 + " " + least + " " + reads;
          }
        }
        """
      Then the property "said" of "root" is "more 3 at least one 1"

    Scenario: A matcher that gives a value that is not a list, or a pattern with no matcher, throws a TypeError
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          function attempt(f) {
            try { return f(); } catch (e) { return e instanceof TypeError ? "type error" : "other"; }
          }
          Component.onCompleted: {
            const Yes = { [Symbol.customMatcher]() { return true; } };
            const plain = { value: 1 };
            said = [attempt(() => match (1) { when Yes(let x): x; default: 0; }),
                    attempt(() => match (1) { when plain(let x): x; default: 0; }),
                    attempt(() => match (1) { when Yes: "yes"; default: "no"; })].join(", ");
          }
        }
        """
      Then the property "said" of "root" is "type error, type error, yes"

  Rule: A match in Ljós's API covers its subject, or has a default

    Scenario: A match that covers every case of a union, an enum or a tagged union has nothing to report
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          enum Access { Undetermined, Granted, Denied }
          property int access: Gauge.Access.Granted
          function word(kind: "start" | "stop", on: boolean): string {
            const a = match (kind) { when "start": "go"; when "stop": "halt"; };
            return a + match (on) { when true: "!"; when false: "."; };
          }
          function say(a: Gauge.Access): string {
            return match (a) {
              when Gauge.Access.Granted: "yes";
              when Gauge.Access.Denied: "no";
              when Gauge.Access.Undetermined: "ask";
            };
          }
          function area(s: { kind: "circle"; r: number } | { kind: "square"; side: number }): number {
            return match (s) {
              when { kind: "circle", let r }: 3 * r * r;
              when { kind: "square", let side }: side * side;
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A match that leaves a case out shows one
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          enum Access { Undetermined, Granted, Denied }
          function word(kind: "start" | "stop" | "pause"): string {
            return match (kind) { when "start": "go"; when "stop": "halt"; };
          }
          function say(a: Gauge.Access): string {
            return match (a) { when Gauge.Access.Granted: "yes"; when Gauge.Access.Denied: "no"; };
          }
          function guarded(on: boolean, ready: boolean): string {
            return match (on) { when true and if (ready): "ready"; when false: "off"; };
          }
          function area(s: { kind: "circle"; r: number } | { kind: "square"; side: number }): number {
            return match (s) { when { kind: "circle", let r }: 3 * r * r; };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:20: error: `match` does not cover every case of `kind`; for example, this case is not handled: `when "pause":` [ts-match-exhaustive]
        Gauge.qml:8:20: error: `match` does not cover every case of `a`; for example, this case is not handled: `when Gauge.Access.Undetermined:` [ts-match-exhaustive]
        Gauge.qml:11:20: error: `match` does not cover every case of `on`; for example, this case is not handled: `when true:` [ts-match-exhaustive]
        Gauge.qml:14:20: error: `match` does not cover every case of `s`; for example, this case is not handled: `when { kind: "square" }:` [ts-match-exhaustive]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: A match on a number or a string needs a default
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property string name: ""
          readonly property string look: match (level) { when if (level < 1): "low"; when 1: "full"; }
          readonly property int size: match (name) { when "": 0; }
          readonly property string fine: match (level) { when if (level < 1): "low"; default: "full"; }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:42: error: `match` on `number` needs a `default:` [ts-match-exhaustive]
        Gauge.qml:6:39: error: `match` on `string` needs a `default:` [ts-match-exhaustive]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A custom matcher in Ljós's API types what its extractor binds
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function name(v: unknown): string {
            const Even = {
              [Symbol.customMatcher](subject: unknown, hint: string): boolean {
                return typeof subject === "number" && subject % 2 === 0;
              }
            };
            const Point = {
              [Symbol.customMatcher](subject: unknown, hint: string): false | number[] {
                return typeof subject === "number" ? [subject, 0] : false;
              }
            };
            return match (v) {
              when Even: "even";
              when Point(let x, 0): "on the axis at " + x.toFixed(1);
              when Point(let x, let y): "at " + (x + y);
              default: "other";
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A custom matcher that gives a Set is taken apart as an array of its items
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function name(v: unknown, seen: Set<number>): string {
            const Seen = {
              [Symbol.customMatcher](subject: unknown, hint: string): false | Set<number> {
                return subject === "seen" ? seen : false;
              }
            };
            return match (v) {
              when Seen(let x, ...): "first seen " + x.toFixed(1);
              default: "other";
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A custom matcher that gives a tuple binds each item as the type at its place
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function name(v: unknown): string {
            const Pair = {
              [Symbol.customMatcher](subject: unknown, hint: string): false | [number, string] {
                return typeof subject === "number" ? [subject, "units"] : false;
              }
            };
            return match (v) {
              when Pair(let x, let unit): x.toFixed(1) + " " + unit.toUpperCase();
              default: "other";
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A custom matcher that gives a string has no list, though a string is iterable
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function name(v: unknown): string {
            const Word = {
              [Symbol.customMatcher](subject: unknown, hint: string): false | string {
                return typeof subject === "string" ? subject : false;
              }
            };
            return match (v) {
              when Word(let c): "starts with " + c;
              default: "other";
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:10:20: error: `Word` gives a string where it matches, not a list to take apart: give an array or another iterable object [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A module that names Symbol.customMatcher with spaces in it is read as one that names it
      Given the file "even.lts":
        """ts
        export const Even = {
          [Symbol
            .customMatcher](subject: unknown, hint: string): boolean {
            return typeof subject === "number" && subject % 2 === 0;
          }
        };
        """
      And the file "tsconfig.json":
        """json
        { "extends": "./.ljos/tsconfig.base.json" }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: An extractor arm removes nothing, so a match of extractors alone needs a default
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function area(s: { kind: "circle"; r: number } | { kind: "square"; side: number }): number {
            const Circle = {
              [Symbol.customMatcher](subject: unknown, hint: string): false | number[] {
                return match (subject) { when { kind: "circle", r: Number and let r }: [r]; default: false; };
              }
            };
            return match (s) {
              when Circle(let r): 3 * r * r;
              when { kind: "square", let side }: side * side;
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:9:20: error: `match` does not cover every case of `s`; for example, this case is not handled: `when { kind: "circle" }:` [ts-match-exhaustive]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A matcher that gives only true or false has no list, so it is no extractor
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function isRow(v: unknown): v is number[] {
            return Array.isArray(v);
          }
          function name(v: unknown): string {
            return match (v) {
              when isRow(let first): "row";
              default: "other";
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:8:20: error: `isRow` gives `true` or `false`, not a list to take apart: name it without `(…)`, or give a list where it matches [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: Symbol.customMatcher is named only as the key of a method in an object literal
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function f(): number {
            const Even = { [Symbol.customMatcher](subject: unknown, hint: string): boolean { return subject === 2; } };
            const it = { [Symbol.iterator](): number { return 1; } };
            const key = Symbol.customMatcher;
            const s = Symbol("x");
            class A {}
            return 2 is Even ? 1 : 0;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:27: error: `Symbol.iterator` is not in Ljós's API: a script names only `Symbol.customMatcher`, `Symbol.dispose` and `Symbol.asyncDispose`, each as the key of a method in an object literal [ts-forbidden]
        Gauge.qml:6:25: error: `Symbol.customMatcher` is not in Ljós's API: it is only the key of a method in an object literal, as in `{ [Symbol.customMatcher](subject: unknown, hint: string): boolean { … } }` [ts-forbidden]
        Gauge.qml:7:23: error: `Symbol` is not in Ljós's API: a script names only `Symbol.customMatcher`, `Symbol.dispose` and `Symbol.asyncDispose`, each as the key of a method in an object literal [ts-forbidden]
        Gauge.qml:8:13: error: `class` is not in Ljós's API: write a function, or a component [ts-forbidden]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: An arm narrows its subject, and is narrows what it tests
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real total: 0
          function measure(s: { kind: "circle"; r: number } | { kind: "square"; side: number }): void {
            if (s is { kind: "circle" }) {
              total = s.r;
            }
            total = match (s) {
              when { kind: "square" }: s.side;
              when { kind: "circle" }: s.r;
            };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A match that awaits is what its arms give once awaited, in an async function and an async generator
      Given the file "pick.lts":
        """ts
        export async function pick(p: Promise<number>, q: Promise<string>): Promise<string> {
          const right: string = match (await p) {
            when 1: "one";
            default: await q;
          };
          const wrong: number = match (await p) {
            when 1: "one";
            default: await q;
          };
          return right + wrong
        }
        export async function* steps(p: Promise<number>, q: Promise<string>): AsyncGenerator<number, string, number> {
          const right: string = match (yield await p) {
            when 1: "one";
            default: await q;
          };
          const wrong: number = match (yield await p) {
            when 1: "one";
            default: await q;
          };
          return right + wrong
        }
        """
      And the file "Gauge.qml":
        """qml
        import Ljos.Ui
        import "pick.lts" as Pick
        Item {}
        """
      And the file "tsconfig.json":
        """json
        { "extends": "./.ljos/tsconfig.base.json" }
        """
      When the files are checked
      Then the check reports:
        """
        pick.lts:6:33: error: `wrong` is a number; this gives it a string [ts-assign-type]
        pick.lts:17:33: error: `wrong` is a number; this gives it a string [ts-assign-type]
        2 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: The text the desugar writes is not held to the rules about how a person writes
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          readonly property bool full: level is 1
          readonly property string first: match ([1, 2]) { when [let a, ...]: "" + a; default: ""; }
          function read(reply: { status: number; body: string }): string {
            if (reply is { status: 200, let body }) {
              return body;
            }
            return "";
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A name is binds may be read only where the is was true
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function read(reply: { status: number; body: string }): string {
            if (reply is { status: 200, let body }) {
              return body;
            }
            return body;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:7:20: error: `body` is bound only where the `is` that binds it was true [ts-is-binding]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A name is binds is its block's, so the block may not declare it too
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function first(x: number): number {
            const y = 1;
            if (x is let y) {
              return y;
            }
            return 0;
          }
          function second(reply: { body: string }, body: string): string {
            if (reply is { let body }) {
              return body;
            }
            return "";
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:5:26: error: `y` is a name of this block already, and an `is` declares what it binds in the block around it: bind another name [ts-is-binding]
        Gauge.qml:11:32: error: `body` is a parameter already, and an `is` declares what it binds in the block around it: bind another name [ts-is-binding]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: What an is in a switch's clause binds is the block's around the switch, so a let beside the switch clashes
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function pick(v: number, a: number): number {
            let x = 0;
            switch (v) {
              case 1: if (a is let x) { return x; } break;
            }
            return 0;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:6:36: error: `x` is a name of this block already, and an `is` declares what it binds in the block around it: bind another name [ts-is-binding]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A name a switch's clauses or a for head declare is not what an is in them may bind, as that would take the is's assignment
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function pick(v: number, a: number): number {
            switch (v) {
              case 1: let x = 1; return x;
              case 2: if (a is let x) { return x; } break;
            }
            return 0;
          }
          function first(xs: number[], a: number): number {
            for (const y of xs) if (a is let y) return y;
            return 0;
          }
          function keep(v: number, a: number): number {
            if (a is let z) { return z; }
            switch (v) {
              case 1: let z = 1; return z;
            }
            return 0;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:6:36: error: `x` is a name of this `switch`'s clauses already, and an `is` in them declares what it binds before the `switch`: bind another name [ts-is-binding]
        Gauge.qml:11:46: error: `y` is a name this `for` declares already, and an `is` in it declares what it binds before the `for`: bind another name [ts-is-binding]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: An is in a for head or a case test is in the scope between too
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function count(a: number): number {
            for (let x = (a is let x) ? 1 : 0; x < 3; x++) { return x; }
            return 0;
          }
          function first(xs: number[], ys: number[]): number {
            for (const y of (xs is let y) ? xs : ys) { return y; }
            return 0;
          }
          function pick(v: number, a: number): number {
            switch (v) {
              case (a is let x) ? 1 : 2: let x = 1; return x;
            }
            return 0;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:36: error: `x` is a name this `for` declares already, and an `is` in it declares what it binds before the `for`: bind another name [ts-is-binding]
        Gauge.qml:8:40: error: `y` is a name this `for` declares already, and an `is` in it declares what it binds before the `for`: bind another name [ts-is-binding]
        Gauge.qml:13:30: error: `x` is a name of this `switch`'s clauses already, and an `is` in them declares what it binds before the `switch`: bind another name [ts-is-binding]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: A catch's parameter is not what an is in its block may bind, as the two would be one name
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function caught(a: number): number {
            try { throw 1; } catch (e) { if (a is let e) { return e; } }
            return 0;
          }
          function message(a: number): number {
            try { throw 1; } catch ({ message }) { if (a is let message) { return message; } }
            return 0;
          }
          function given(a: number): number {
            try { throw 1; } catch ({ m = a is let m }) { return 0; }
            return 0;
          }
          function deeper(a: number): number {
            try { throw 1; } catch (e) { { if (a is let e) { return e; } } }
            return 0;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:55: error: `e` is a parameter already, and an `is` declares what it binds in the block around it: bind another name [ts-is-binding]
        Gauge.qml:8:65: error: `message` is a parameter already, and an `is` declares what it binds in the block around it: bind another name [ts-is-binding]
        Gauge.qml:12:52: error: `m` is a name this `catch` declares already, and an `is` in its parameter declares what it binds before the `try`: bind another name [ts-is-binding]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: Nested switches that both declare the name say the is once, at the inner one
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function inner(v: number, w: number, a: number): number {
            switch (v) {
              case 1: let x = 1;
                switch (w) {
                  case 2: let x = 2; if (a is let x) { return x; }
                }
            }
            return 0;
          }
          function outer(v: number, w: number, a: number): number {
            switch (v) {
              case 1: let x = 1;
                switch (w) {
                  case 2: if (a is let x) { return x; }
                }
            }
            return 0;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:7:51: error: `x` is a name of this `switch`'s clauses already, and an `is` in them declares what it binds before the `switch`: bind another name [ts-is-binding]
        Gauge.qml:16:40: error: `x` is a name of this `switch`'s clauses already, and an `is` in them declares what it binds before the `switch`: bind another name [ts-is-binding]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A name a match arm binds is not what an is in its guard or its value may bind
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function guarded(v: number, a: number): number {
            return match (v) { when let n and if (a is let n): n; default: 0; };
          }
          function valued(v: number, a: number): number {
            return match (v) { when let n: (a is let n) ? n : 0; default: 0; };
          }
          function other(v: number, a: number): number {
            return match (v) { when let n: (a is let m) ? m + n : 0; default: 0; };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:60: error: `n` is a name this `match` arm binds already, and an `is` in the arm declares what it binds before the `match`: bind another name [ts-is-binding]
        Gauge.qml:7:54: error: `n` is a name this `match` arm binds already, and an `is` in the arm declares what it binds before the `match`: bind another name [ts-is-binding]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A function's parameter is not what an is in its parameters, or in an arrow's body that is not a block, may bind
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function defaulted(a: number): number {
            function pick(x: number, b: boolean = a is let x): number { return b ? x : 0; }
            return pick(1);
          }
          function arrow(a: number): boolean {
            const test = (x: number): boolean => a is let x;
            return test(1);
          }
          function named(a: number): number {
            const f = function x(b: boolean = a is let x): number { return 1; };
            return f();
          }
          function apart(a: number): boolean {
            const test = (y: number): boolean => a is let x;
            return test(1);
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:60: error: `x` is a name this function's head declares already, and an `is` outside its block declares what it binds before the function: bind another name [ts-is-binding]
        Gauge.qml:8:59: error: `x` is a name this function's head declares already, and an `is` outside its block declares what it binds before the function: bind another name [ts-is-binding]
        Gauge.qml:12:56: error: `x` is a name this function's head declares already, and an `is` outside its block declares what it binds before the function: bind another name [ts-is-binding]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    Scenario: A name a module imports or declares is not what an is at its top may bind
      Given the file "other.lts":
        """ts
        export const other: number = 1
        """
      And the file "pick.lts":
        """ts
        import { other } from "./other.lts"
        export const own: number = 3
        const a: number = 7
        if (a is let other) { }
        if (a is let own) { }
        if (a is let fresh) { }
        export function run(): number { return 0 }
        """
      And the file "Gauge.qml":
        """qml
        import Ljos.Ui
        import "pick.lts" as Pick
        Item {
          property real picked: Pick.run()
        }
        """
      And the file "tsconfig.json":
        """json
        { "extends": "./.ljos/tsconfig.base.json" }
        """
      When the files are checked
      Then the check reports:
        """
        pick.lts:4:22: error: `other` is a name of this block already, and an `is` declares what it binds in the block around it: bind another name [ts-is-binding]
        pick.lts:5:22: error: `own` is a name of this block already, and an `is` declares what it binds in the block around it: bind another name [ts-is-binding]
        3 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

  Rule: What the proposal leaves out is an error that says so

    Scenario: A name that is not in scope, an extractor of one and a relational pattern are errors
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          readonly property int a: match (level) { when picked: 1; default: 0; }
          readonly property int b: match (level) { when Some(let x): 1; default: 0; }
          readonly property int c: match (level) { when < 10: 1; default: 0; }
          readonly property bool d: "x" is /x/g
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:6:57: error: a relational pattern is not in the proposal: write a guard, as in `when let n and if (n < 10):` [syntax]
        Gauge.qml:7:44: error: a regular expression with the `g` or `y` flag keeps its place between tests: leave the flag out [syntax]
        Gauge.qml:4:57: error: `picked` is not defined; to bind the value, write `let picked` [ts-no-any]
        Gauge.qml:5:57: error: `Some` is not defined; to bind the value, write `let Some` [ts-no-any]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: An extractor's list that is not closed is a syntax error
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          readonly property int a: match (level) { when Point(let x: 1; default: 0; }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:68: error: Expected `)` but found `:` [syntax]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: A new.target in a match that yields is an error, as the generator it is lowered to has its own
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          function* steps(): Generator<number, void, number> {
            const n = match (yield 1) { when 1: new.target; default: 0; };
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:49: error: a `match` or `is` that yields is lowered to a generator, which has no `super` or `new.target` of the function around it: read it into a name before, as in `const base = super.size;` [syntax]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    Scenario: and and or mixed need parentheses
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          readonly property int a: match (level) { when 1 and 2 or 3: 1; default: 0; }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Gauge.qml:4:65: error: `and` and `or` mixed need parentheses, as in `(a and b) or c` [syntax]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """
