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
