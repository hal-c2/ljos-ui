# Sources:
#   docs/DIALECT.md (§4.17.1 the pipe, §4.21 the desugar, §5.1 `ts-lts-syntax`)
#   https://github.com/tc39/proposal-pipeline-operator (its README and spec as of 2025-10-03)
# The pipe is TC39's Hack pipe, which Qt's QML does not have: there is no oracle.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Handing a value from step to step with the pipe
  `head |> body` reads `head` once, then gives `body`'s value, where `%`
  in the body is what the head gave. The steps read in the order they
  run: `a |> f(%) |> g(%)` is `g(f(a))`. A body must use `%`; `%` means
  nothing outside a body; and a body that is an arrow, a conditional,
  an assignment or a `yield` is written in parentheses. `%` after an
  operand is still the remainder. The checker types `%` as its head.

  Rule: Each step reads the value of the step before it

    Scenario: A discount is the difference, kept at zero or more, as a share, rounded
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property real oldPrice: 40
          property real newPrice: 30
          readonly property real discountPercent:
            oldPrice - newPrice |> Math.max(%, 0) / oldPrice * 100 |> Math.round(%)
        }
        """
      Then the property "discountPercent" of "root" is 25
      When the property "newPrice" of "root" is set to 50
      Then the property "discountPercent" of "root" is 0
      When the property "oldPrice" of "root" is set to 75
      And the property "newPrice" of "root" is set to 50
      Then the property "discountPercent" of "root" is 33

    Scenario: How long ago something came out is the minutes piped into a match of guards
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property real age: 90
          readonly property string said: timeSinceEvent("2026-01-01T12:00", Date.parse("2026-01-01T12:00") + age * 60000)
          function timeSinceEvent(published: string, now: number): string {
            return (now - Date.parse(published)) / 60000 |> Math.floor(%) |> match (%) {
              when if (% < 2): qsTr("Just now");
              when if (% < 60): qsTr("%1 minutes ago").arg(%);
              when if (% < 120): qsTr("1 hour ago");
              when if (% < 1440): Math.floor(% / 60) |> qsTr("%1 hours ago").arg(%);
              default: Locale.current.formatDate(published, "yyyy-MM-dd");
            }
          }
        }
        """
      Then the property "said" of "root" is "1 hour ago"
      When the property "age" of "root" is set to 0
      Then the property "said" of "root" is "Just now"
      When the property "age" of "root" is set to 3
      Then the property "said" of "root" is "3 minutes ago"
      When the property "age" of "root" is set to 300
      Then the property "said" of "root" is "5 hours ago"
      When the property "age" of "root" is set to 3000
      Then the property "said" of "root" is "2026-01-01"

    Scenario: A body is any expression that uses `%`, and `%` after an operand is the remainder
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property int n: 7
          readonly property int left: n |> % + 1 |> % % 3
          readonly property string listed: [1, 2, 3] |> %.map(x => x * 2) |> %.join(",")
          readonly property string shown: "ljos" |> `${%}!` |> [%, %].join(" ")
          readonly property real kept: n |> (% > 5 ? % : 0)
        }
        """
      Then the property "left" of "root" is 2
      And the property "listed" of "root" is "2,4,6"
      And the property "shown" of "root" is "ljos! ljos!"
      And the property "kept" of "root" is 7
      When the property "n" of "root" is set to 3
      Then the property "left" of "root" is 1
      And the property "kept" of "root" is 0

    Scenario: The head is read once, however many times the body reads `%`
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property int calls: 0
          property int got: 0
          function next(): number { calls += 1; return calls * 10 }
          Component.onCompleted: got = next() |> % + % + %
        }
        """
      Then the property "got" of "root" is 30
      And the property "calls" of "root" is 1

    Scenario: A pipe in a body has its own topic, its head reads the outer one, and a closure keeps the topic it saw
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          readonly property int nested: 3 |> (% + 1 |> % * 10) + %
          readonly property int chained: 2 |> % + 1 |> % * %
          readonly property string kept: [1, 2, 3].map(n => n |> (() => % * 10)) |> %.map(f => f()).join(",")
        }
        """
      Then the property "nested" of "root" is 43
      And the property "chained" of "root" is 9
      And the property "kept" of "root" is "10,20,30"

    Scenario: A pipe as a statement, after a line with no semicolon
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string said: ""
          function step() {
            said = "a"
            said |> (said = % + "b")
          }
          Component.onCompleted: step()
        }
        """
      Then the property "said" of "root" is "ab"

    Scenario: A pipe may await in its head and its bodies, and reads in order
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string said: ""
          function size(p: Promise<number>): Promise<string> {
            return (async () => {
              const log: string[] = []
              const n = await p |> (log.push("head"), % + 1) |> await Promise.resolve(% * 2)
              log.push("after")
              return `${n} ${log.join(",")}`
            })()
          }
          Component.onCompleted: size(Promise.resolve(3)).then(v => said = v)
        }
        """
      When 10 milliseconds pass
      Then the property "said" of "root" is "8 head,after"

    Scenario: A pipe in a generator yields from a body in parentheses
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string said: ""
          function steps(k: number): Generator<number, number, number> {
            return (function* () {
              const n = k |> (yield %) |> % * 2
              return n
            })()
          }
          Component.onCompleted: {
            const run = steps(3)
            const first = run.next().value
            const last = run.next(5)
            said = `${first} ${last.value} ${last.done}`
          }
        }
        """
      Then the property "said" of "root" is "3 10 true"

    Scenario: A pipe in an `.lts` module compiles and runs
      Given the file "size.lts":
        """ts
        export function half(n: number): number {
          return n |> % / 2
        }
        """
      And the QML:
        """qml
        import Ljos.Ui
        import "size.lts" as Size
        Item {
          objectName: "root"
          property real gap: Size.half(24) |> % + 1
        }
        """
      Then the property "gap" of "root" is 13

    Scenario: A name the file writes is never one the lowering declares
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property int n: 7
          readonly property string said: both()
          function both(): string {
            const $0 = "zero"
            const $s = "subject"
            const piped = n |> `${%} ${$0}`
            const matched = match (n + 1) { when 8: $s; default: "none" }
            return `${piped} ${matched}`
          }
        }
        """
      Then the property "said" of "root" is "7 zero subject"

    Scenario: A name spelled with an escape is the name it spells
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          readonly property int twelve: escaped()
          function escaped(): number {
            const \u{24}0/*€*/ = 10
            return 2 |> % + \u{24}0/*€*/
          }
        }
        """
      Then the property "twelve" of "root" is 12

  Rule: The checker types `%` as the head, and each step as its body

    Scenario: The examples check clean, and the text the desugar writes is not held to the rules about how a person writes
      Given the file "Card.qml":
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          property real oldPrice: 40
          property real newPrice: 30
          readonly property real discountPercent:
            oldPrice - newPrice |> Math.max(%, 0) / oldPrice * 100 |> Math.round(%)
          readonly property string nested: oldPrice |> (% + 1 |> `${%}`) + "!"
          function timeSinceEvent(published: string, now: number): string {
            return (now - Date.parse(published)) / 60000 |> Math.floor(%) |> match (%) {
              when if (% < 2): qsTr("Just now");
              when if (% < 60): qsTr("%1 minutes ago").arg(%);
              when if (% < 120): qsTr("1 hour ago");
              when if (% < 1440): Math.floor(% / 60) |> qsTr("%1 hours ago").arg(%);
              default: Locale.current.formatDate(published, "yyyy-MM-dd");
            }
          }
          function load(p: Promise<number>): Promise<string> {
            return (async () => await p |> % * 2 |> `${%}`)();
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: `%` has the head's type, and a step its body's
      Given the file "Card.qml":
        """qml
        import Ljos.Ui
        Item {
          readonly property real top: "3" |> Math.max(%, 0)
          readonly property string shown: 4 |> % + 1
          readonly property real fine: "3" |> %.length |> Math.max(%, 0)
        }
        """
      When the files are checked
      Then the check reports:
        """
        Card.qml:3:55: error: `rest` is a number; this gives it a string [ts-assign-type]
        Card.qml:4:43: error: `shown` is a string; this gives it a number [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A pipe in a `.ts` module is an error that names the rename
      Given the file "size.ts" without its indentation:
        """ts
        export function half(n: number): number {
          return n |> % / 2
        }
        """
      And the file "Pad.qml":
        """qml
        import Ljos.Ui
        import "size.ts" as Size
        Item {
          property real gap: Size.half(8)
        }
        """
      When the files are checked
      Then the check reports:
        """
        size.ts:2:12: error: `|>` is Ljós syntax, which stock TypeScript tools cannot read in a `.ts` file; rename `size.ts` to `size.lts` [ts-lts-syntax]
        2 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

  Rule: What the proposal forbids is an error that says so

    Scenario: A body that does not use `%`, and `%` outside a body
      Given the file "Card.qml":
        """qml
        import Ljos.Ui
        Item {
          property real n: 2
          readonly property real a: n |> 1
          readonly property real b: n |> % + 1 |> 2
          readonly property real c: % + 1
          function d(): number { return n |> (() => 1)() }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Card.qml:4:42: error: the pipe's body does not use its topic `%` [syntax]
        Card.qml:5:51: error: the pipe's body does not use its topic `%` [syntax]
        Card.qml:6:37: error: `%` is the topic of a pipe's body, and there is no pipe here: write it after `|>`, as in `x |> f(%)` [syntax]
        Card.qml:7:46: error: the pipe's body does not use its topic `%` [syntax]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: A body that is an arrow, a conditional, an assignment or a yield needs parentheses
      Given the file "Card.qml":
        """qml
        import Ljos.Ui
        Item {
          property real n: 2
          readonly property unknown a: n |> () => %
          readonly property real b: n |> % > 1 ? % : 0
          function c(): void { n |> n = % }
          function* d(): Generator<number, void, number> { n |> yield % }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Card.qml:4:45: error: an arrow function as a pipe's body is written in parentheses: `|> (() => %)` [syntax]
        Card.qml:5:42: error: a conditional as a pipe's body is written in parentheses: `|> (% > 0 ? % : 0)` [syntax]
        Card.qml:6:37: error: an assignment as a pipe's body is written in parentheses: `|> (total = %)` [syntax]
        Card.qml:7:65: error: a `yield` as a pipe's body is written in parentheses: `|> (yield %)` [syntax]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """
