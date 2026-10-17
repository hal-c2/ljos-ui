# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qml-qtquick-positioners-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
# Verified on Qt 6.11.2 (qml6 and qmltestrunner, offscreen): the
# children are placed when `Component.onCompleted` runs, and a change made
# later is placed with the next frame, not at once.

@qt-parity
Feature: A positioner has placed its children once it is complete
  A Row, Column, Grid or Flow places its children, and takes its implicit
  size, as it is completed: a script run on completion, or one that made
  the positioner with `createObject`, reads the places. A change made
  after that is placed before the next frame. So a child anchored to fill
  its Column, which stops the Column from working, does not undo the
  places the others got while it had no size. Positioners inside
  positioners, and children a Repeater makes, are placed the same way.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string log: ""
        Column { id: col; Rectangle { width: 10; height: 10 } Rectangle { id: c2; width: 10; height: 10 } }
        Row { id: row; Rectangle { width: 10; height: 10 } Rectangle { id: r2; width: 10; height: 10 } }
        Grid { id: grid; columns: 2; Rectangle { width: 10; height: 10 } Rectangle { id: g2; width: 10; height: 10 } }
        Flow { width: 100; Rectangle { width: 10; height: 10 } Rectangle { id: f2; width: 10; height: 10 } }
        Column { Row { Rectangle { width: 10; height: 10 } Rectangle { width: 10; height: 15 } }
          Rectangle { id: below; width: 10; height: 10 }
          Column { Repeater { model: 3; Rectangle { width: 10; height: 10 } } }
          Rectangle { id: last; width: 10; height: 10 }
        }
        Column {
          id: filled; spacing: 2; Rectangle { width: 50; height: 50 }
          Rectangle { objectName: "green"; width: 20; height: 50 } MouseArea { anchors.fill: parent }
        }
        Component.onCompleted: {
          log = [c2.y, r2.x, g2.x, f2.x, col.height, row.width, below.y, last.y].join(" ")
          filled.width = 400; filled.height = 300
        }
        Component { id: made
          Column { property alias second: s; Rectangle { width: 10; height: 10 } Rectangle { id: s; width: 10; height: 10 } } }
        function make() { var c = made.createObject(root); log = c.second.y + " " + c.height }
      }
      """

  @api-QtQuick.Component.completed
  Scenario: A script run on completion reads the places, in positioners inside positioners too
    Then the property "log" of "root" is "10 10 10 10 20 20 15 55"

  Scenario: A positioner made by a script is placed when it is returned
    When the method "make" of "root" is called
    Then the property "log" of "root" is "10 20"

  @api-QtQuick.Column
  Scenario: A child that comes to fill its Column keeps the others where they were
    When 100 milliseconds pass
    Then the property "y" of "green" is 52
    And the log has a warning matching "Column will not function"
