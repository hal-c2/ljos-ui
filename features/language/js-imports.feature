# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-imports.html
#   https://doc.qt.io/qt-6/qtqml-javascript-resources.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: JavaScript files imported by name
  A script file imported with `import "file.js" as Name` gives its functions
  to the importing file as members of `Name`, and what its functions are
  handed they can call.

  Background:
    Given the file "Panel.qml":
      """qml
      import QtQuick
      Item {
        property int count: 0
        function isEmpty() { return count === 0 }
      }
      """
    And the file "rules.js":
      """
      let calls = 0
      function disabled(op, panel) {
        calls = calls + 1
        if (op === "x" && panel.isEmpty())
          return true
        return false
      }
      function callCount() { return calls }
      """

  Scenario: A script function calls a function of an item it was given
    Given the QML:
      """qml
      import QtQuick
      import "rules.js" as Rules
      Item {
        objectName: "root"
        Panel { id: panel }
        property bool empty: Rules.disabled("x", panel)
        property bool other: Rules.disabled("y", panel)
        property int calls: Rules.callCount()
      }
      """
    Then the property "empty" of "root" is true
    And the property "other" of "root" is false
    And the property "calls" of "root" is at least 2

  Scenario: The same from a component that holds the item as a required property
    Given the file "Gate.qml":
      """qml
      import QtQml
      import "rules.js" as Rules
      QtObject {
        required property Panel panel
        function disabled(op) { return Rules.disabled(op, panel) }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Panel { id: panel }
        Gate { id: gate; panel: panel }
        property bool empty: gate.disabled("x")
      }
      """
    Then the property "empty" of "root" is true

  Rule: What a script keeps is its importer's own, unless it is a library

    Background:
      Given the file "counter.js":
        """
        var total = 0
        let hidden = 5
        function bump() { total = total + 1; return total }
        function readHidden() { return hidden }
        """
      And the file "shared.js":
        """
        .pragma library
        var total = 0
        function bump() { total = total + 1; return total }
        """
      And the file "Bumper.qml":
        """qml
        import QtQuick
        import "counter.js" as Own
        import "shared.js" as Shared
        Item {
          property int own: 0
          property int shared: 0
          function bumpBoth() { own = Own.bump(); shared = Shared.bump() }
        }
        """

    Scenario: Each instance of a component has its own copy of a script
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Bumper { id: one }
          Bumper { id: two }
          Component.onCompleted: { one.bumpBoth(); one.bumpBoth(); two.bumpBoth() }
          property int oneOwn: one.own
          property int twoOwn: two.own
        }
        """
      Then the property "oneOwn" of "root" is 2
      And the property "twoOwn" of "root" is 1

    Scenario: A library script is one for every importer
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Bumper { id: one }
          Bumper { id: two }
          Component.onCompleted: { one.bumpBoth(); one.bumpBoth(); two.bumpBoth() }
          property int oneShared: one.shared
          property int twoShared: two.shared
        }
        """
      Then the property "oneShared" of "root" is 2
      And the property "twoShared" of "root" is 3

    Scenario: What a script declared at its top level is a member of its name
      Given the QML:
        """qml
        import QtQuick
        import "counter.js" as Own
        Item {
          objectName: "root"
          property int hidden: Own.readHidden()
          property int total: Own.total
          property int declared: Own.hidden
        }
        """
      Then the property "hidden" of "root" is 5
      And the property "total" of "root" is 0
      And the property "declared" of "root" is 5

  Rule: A script can import a module under a name

    Background:
      Given the file "status.js":
        """
        .pragma library
        .import QtQuick as QQ
        function errorStatus() { return QQ.Component.Error }
        function alignment() { return QQ.Text.AlignHCenter }
        function unknown() { return typeof QQ.Nothing }
        """
      And the file "plain.js":
        """
        .import QtQuick as Q
        function ready() { return Q.Component.Ready }
        """
      And the QML:
        """qml
        import QtQuick
        import "status.js" as Status
        import "plain.js" as Plain
        Item {
          objectName: "root"
          property int error: Status.errorStatus()
          property int alignment: Status.alignment()
          property string unknown: Status.unknown()
          property string alias: typeof Status.QQ
          property int ready: Plain.ready()
        }
        """

    Scenario Outline: The module's types are read through the name
      Then the property "<property>" of "root" is <value>

      Examples:
        | property  | value       |
        | error     | 3           |
        | alignment | 4           |
        | unknown   | "undefined" |
        | ready     | 1           |

    Scenario: The name is the script's own, not a member of it
      Then the property "alias" of "root" is "undefined"

  Rule: A script can import another script under a name

    Background:
      Given the file "js/lib.js":
        """
        .pragma library
        var total = 0
        function bump() { total = total + 1; return total }
        function dbl(n) { return n * 2 }
        """
      And the file "js/own.js":
        """
        var total = 0
        function bump() { total = total + 1; return total }
        """
      And the file "js/user.js":
        """
        .import "lib.js" as L
        .import "own.js" as O

        function twice(n) { return L.dbl(n) }
        function bumpLib() { return L.bump() }
        function bumpOwn() { return O.bump() }
        """
      And the file "User.qml":
        """qml
        import QtQuick
        import "js/user.js" as U
        Item {
          function bumpLib() { return U.bumpLib() }
          function bumpOwn() { return U.bumpOwn() }
        }
        """

    Scenario: The imported script's functions are called through its name
      Given the QML:
        """qml
        import QtQuick
        import "js/user.js" as U
        Item {
          objectName: "root"
          property int twice: U.twice(21)
          property string imported: typeof U.L
        }
        """
      Then the property "twice" of "root" is 42
      And the property "imported" of "root" is "undefined"

    Scenario: A library a script imports is the one every importer has
      Given the QML:
        """qml
        import QtQuick
        import "js/lib.js" as L
        Item {
          objectName: "root"
          User { id: one }
          User { id: two }
          property var counts: [one.bumpLib(), one.bumpLib(), two.bumpLib(), L.bump()]
        }
        """
      Then the property "counts" of "root" is [1, 2, 3, 4]

    Scenario: A script that is not a library is the importing script's own
      Given the QML:
        """qml
        import QtQuick
        import "js/own.js" as O
        Item {
          objectName: "root"
          User { id: one }
          User { id: two }
          property var counts: [one.bumpOwn(), one.bumpOwn(), two.bumpOwn(), O.bump()]
        }
        """
      Then the property "counts" of "root" is [1, 2, 1, 1]

    Scenario: A script that imports a script that is not there does not load
      Given the file "js/broken.js":
        """
        .import "missing.js" as M
        function f() { return 1 }
        """
      And the file "Broken.qml":
        """qml
        import QtQuick
        import "js/broken.js" as B
        Item { property int f: B.f() }
        """
      Then loading "Broken.qml" is rejected

    Scenario: Two scripts that import each other do not load
      Given the file "js/x.js":
        """
        .import "y.js" as Y
        function fx() { return "x" }
        """
      And the file "js/y.js":
        """
        .import "x.js" as X
        function fy() { return "y" }
        """
      And the file "Cycle.qml":
        """qml
        import QtQuick
        import "js/x.js" as X
        Item { property string f: X.fx() }
        """
      Then loading "Cycle.qml" is rejected
