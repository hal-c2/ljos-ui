# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathattribute.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathpercent.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathview.html (path, the attached properties)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PathAttribute @api-QtQuick.PathPercent
Feature: What a path tells the delegates along it
  A `PathAttribute` names a value at a point of the path; each delegate reads it, attached to the
  `PathView`, as it is where the delegate is, changing evenly between the points that name it. A
  `PathPercent` says how much of the rows lie before its point.

  Scenario: An attribute changes evenly between where the path names it
    Given the QML:
      """qml
      import QtQuick
      PathView {
        width: 500; height: 300
        model: 8
        delegate: Item {
          objectName: "d" + index
          width: 20; height: 10
          property var a: PathView.a
          property var b: PathView.b
          property bool none: PathView.zz === undefined
        }
        path: Path {
          startX: 0; startY: 50
          PathAttribute { name: "a"; value: 2 }
          PathLine { x: 200; y: 50 }
          PathAttribute { name: "a"; value: 4 }
          PathLine { x: 400; y: 50 }
          PathAttribute { name: "b"; value: 2 }
        }
      }
      """
    Then the property "a" of "d0" is 2
    And the property "a" of "d1" is 2.5
    And the property "a" of "d4" is 4
    And the property "a" of "d6" is 3
    And the property "b" of "d0" is 0
    And the property "b" of "d2" is 0.5
    And the property "b" of "d4" is 1
    And the property "none" of "d3" is true

  Scenario: A percentage crowds the rows before its point
    Given the QML:
      """qml
      import QtQuick
      PathView {
        objectName: "pv"
        width: 500; height: 300
        model: 4
        highlightRangeMode: PathView.NoHighlightRange
        delegate: Item {
          objectName: "d" + index
          width: 20; height: 10
          property var a: PathView.a
          property var b: PathView.b
        }
        path: Path {
          startX: 0; startY: 50
          PathAttribute { name: "a"; value: 1 }
          PathLine { x: 100; y: 50 }
          PathPercent { value: 0.5 }
          PathAttribute { name: "b"; value: 10 }
          PathLine { x: 400; y: 50 }
          PathAttribute { name: "a"; value: 3 }
          PathAttribute { name: "b"; value: 20 }
        }
      }
      """
    Then the property "x" of "d1" is 40
    And the property "x" of "d2" is 90
    And the property "x" of "d3" is 240
    And the property "a" of "d1" is 1.25
    And the property "b" of "d1" is 5
    And the property "b" of "d3" is 15
    When the property "offset" of "pv" is set to 2.4
    Then the property "x" of "d0" is within 0.001 of 150
    And the property "x" of "d3" is within 0.001 of 60
    And the property "a" of "d3" is within 0.001 of 1.35
