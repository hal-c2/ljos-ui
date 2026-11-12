# Sources:
#   docs/DIALECT.md (§4.18 `match` and `is`, §4.21 the desugar, §5.1 `ts-match-exhaustive` and `ts-is-binding`)
#   https://github.com/tc39/proposal-pattern-matching (its README as of 2025-10-03)
# `match` and `is` are TC39's pattern matching, which Qt's QML does not have: there is no oracle.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Choosing a value with match, and testing one with is
  A script says "which of these is it?" with `match`: its arms are tried
  in order, and the first whose pattern matches the subject gives the
  value. A pattern is a literal, a name to compare with or call, `String`,
  `Number` or `Boolean`, a regular expression, a binding (`let x`), an
  array, an object, or `and`, `or` and `not` of those, and `if (c)` is a
  pattern that holds where `c` does. `x is pattern` is the same test as a
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

  Rule: What the proposal leaves out is an error that says so

    Scenario: A name that is not in scope, an extractor and a relational pattern are errors
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
        Gauge.qml:5:57: error: an extractor is not in Ljós's `match`: test the value with a predicate, as in `when let x and isSome:` [syntax]
        Gauge.qml:6:57: error: a relational pattern is not in the proposal: write a guard, as in `when let n and if (n < 10):` [syntax]
        Gauge.qml:7:44: error: a regular expression with the `g` or `y` flag keeps its place between tests: leave the flag out [syntax]
        Gauge.qml:4:57: error: `picked` is not defined; to bind the value, write `let picked` [ts-no-any]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
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
