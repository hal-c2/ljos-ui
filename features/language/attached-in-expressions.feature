# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#attached-properties-and-attached-signal-handlers
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Attached properties in expressions
  An attached property can be read and written in a binding or a handler: by the type's name
  alone for the object the expression is written in, and after another object for that object.

  Rule: The type's name alone means the object the expression is written in

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          Item {
            objectName: "a"
            Layout.fillWidth: true
            Layout.row: 2
            property bool fills: Layout.fillWidth
            property int doubled: Layout.row * 2
            property int column: Layout.column
            property real wanted: Layout.preferredWidth
            function move() { Layout.row = 5 }
            function inner() { return [1].map(() => Layout.row)[0] }
            property int seen: -1
            function look() { seen = inner() }
          }
          Item { objectName: "b"; Layout.row: 9; property int mine: Layout.row }
        }
        """

    Scenario: A binding reads the attached properties of its own object
      Then the property "fills" of "a" is true
      And the property "doubled" of "a" is 4
      And the property "mine" of "b" is 9

    Scenario: An attached property that was never set reads its default
      Then the property "column" of "a" is 0
      And the property "wanted" of "a" is -1

    Scenario: A handler writes an attached property and bindings on it follow
      When the method "move" of "a" is called
      Then the property "doubled" of "a" is 10
      And the property "mine" of "b" is 9

    Scenario: A function inside a function still means the same object
      When the method "look" of "a" is called
      Then the property "seen" of "a" is 2

  Rule: After another object it means that object

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        GridLayout {
          id: grid
          objectName: "grid"
          Item { id: first; objectName: "first"; Layout.row: 1; Layout.column: 3 }
          Item {
            objectName: "second"
            Layout.row: 4
            property int theirs: first.Layout.row
            property int sum: first.Layout.row + first.Layout.column + Layout.row
            property int viaParent: parent.Layout.row
            property int unset: grid.Layout.columnSpan
            function push() { first.Layout.row = 6 }
            function pushParent() { parent.Layout.row = 8 }
          }
        }
        """

    Scenario: A binding reads the attached properties of another object
      Then the property "theirs" of "second" is 1
      And the property "sum" of "second" is 8
      And the property "viaParent" of "second" is 0
      And the property "unset" of "second" is 1

    Scenario: Writing through another object changes that object's attached property
      When the method "push" of "second" is called
      Then the property "theirs" of "second" is 6
      And the property "sum" of "second" is 13

    Scenario: Writing through an object that had none makes it
      When the method "pushParent" of "second" is called
      Then the property "viaParent" of "second" is 8

  Rule: Named through a namespace, after another object it still means that object

    Background:
      Given the QML:
        """qml
        import QtQuick as Q
        import QtQuick.Layouts as L
        import QtQuick.Controls as T
        L.ColumnLayout {
          objectName: "column"
          Q.Item { id: a; L.Layout.fillWidth: true; L.Layout.preferredHeight: 12 }
          Q.Item { id: k; Q.Keys.enabled: false }
          T.Button { id: b; T.ToolTip.text: "tip" }
          property bool fills: a.L.Layout.fillWidth
          property int tall: a.L.Layout.preferredHeight
          property bool keys: k.Q.Keys.enabled
          property string tip: b.T.ToolTip.text
          function grow() { a.L.Layout.preferredHeight = 20 }
        }
        """

    Scenario: A binding reads the attached properties of another object through a namespace
      Then the property "fills" of "column" is true
      And the property "tall" of "column" is 12
      And the property "keys" of "column" is false
      And the property "tip" of "column" is "tip"

    Scenario: Writing through a namespace changes that object's attached property
      When the method "grow" of "column" is called
      Then the property "tall" of "column" is 20

  Rule: An attaching type keeps its enumeration next to its attached properties

    Scenario: A name and a role are read and written through Accessible
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Item { id: save; Accessible.name: "Save"; Accessible.role: Accessible.Button }
          property string label: save.Accessible.name
          property bool isButton: save.Accessible.role === Accessible.Button
          property bool isLink: save.Accessible.role === Accessible.Link
          function rename() { save.Accessible.name = "Open" }
          function turn() { save.Accessible.role = Accessible.Link }
        }
        """
      Then the property "label" of "root" is "Save"
      And the property "isButton" of "root" is true
      When the method "rename" of "root" is called
      And the method "turn" of "root" is called
      Then the property "label" of "root" is "Open"
      And the property "isButton" of "root" is false
      And the property "isLink" of "root" is true

  Rule: The type's name alone, as a value, is the object it attaches

    Scenario: Each object has its own attached object
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        RowLayout {
          objectName: "root"
          property QtObject mine: Layout
          Item { id: child; Layout.row: 3; property QtObject mine: Layout }
          property bool distinct: mine !== child.mine
          property int row: child.mine.row
          function same() { let l = Layout; return l === mine }
          property bool seen: false
          function look() { seen = same() }
        }
        """
      When the method "look" of "root" is called
      Then the property "distinct" of "root" is true
      And the property "row" of "root" is 3
      And the property "seen" of "root" is true
