# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#model-prop
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
#   https://github.com/olafura/ljos-ui/issues/125
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A view given a model equal to its own keeps its delegates
  A view given a new JS array, or a number, equal to the model it shows keeps the delegates it made:
  arrays are equal when their items are, objects in them when their fields are, in any order, and
  items compare loosely. A model that differs makes every delegate again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQml
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property var m: [1, 2]
        property var made: ({ r: 0, l: 0, g: 0, p: 0, t: 0, i: 0 })
        property string seen
        function mk(v) { made[v]++ }
        function look() { seen = Object.values(made).join(",") }
        Repeater { model: root.m; Item { Component.onCompleted: root.mk("r") } }
        ListView { width: 50; height: 50; model: root.m; delegate: Item { width: 9; height: 9; Component.onCompleted: root.mk("l") } }
        GridView { width: 50; height: 50; cellWidth: 9; cellHeight: 9; model: root.m; delegate: Item { Component.onCompleted: root.mk("g") } }
        PathView { model: root.m; path: Path { PathLine { x: 50; y: 50 } } delegate: Item { Component.onCompleted: root.mk("p") } }
        TableView { width: 50; height: 50; model: root.m; delegate: Item { implicitWidth: 9; implicitHeight: 9; Component.onCompleted: root.mk("t") } }
        Instantiator { model: root.m; QtObject { Component.onCompleted: root.mk("i") } }
        function give(next) { const was = made.r; m = next; return made.r - was }
        function run(steps) { seen = steps.map((s) => give(s[0]) + "/" + give(s[1])).join(" ") }
        function loosely() { run([[[1], [true]], [[0], ["0"]], [[1], ["1.0"]], [[0], [""]], [[null], [undefined]], [[1, 2], [2, 1]]]) }
        function objects() { run([[[{ x: 1, y: 2 }], [{ y: 2, x: "1" }]], [[{ x: 1 }], [{ x: 1, y: 2 }]], [[1, [2]], [1, [2]]]]) }
        function numbers() { run([[3, 3.0], [3, "3"], [3, [3]], [[], []]]) }
      }
      """

  Scenario: Every view keeps its delegates for an equal array, and remakes them for another
    When the property "m" of "root" is set to [1, 2]
    And 50 milliseconds pass
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "2,2,2,2,2,2"
    When the property "m" of "root" is set to [3, 4]
    And 50 milliseconds pass
    And the method "look" of "root" is called
    Then the property "seen" of "root" is "4,4,4,4,4,4"

  Scenario: Arrays are equal when their items are, loosely
    When the method "loosely" of "root" is called
    Then the property "seen" of "root" is "1/0 1/0 1/1 1/1 1/1 2/2"

  Scenario: Objects in arrays are equal when their fields are, in any order
    When the method "objects" of "root" is called
    Then the property "seen" of "root" is "1/0 1/1 2/0"

  Scenario: A number is equal to the same number, not to an array of it
    When the method "numbers" of "root" is called
    Then the property "seen" of "root" is "3/0 0/0 0/1 0/0"
