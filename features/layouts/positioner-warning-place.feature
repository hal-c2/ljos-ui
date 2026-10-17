# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-column.html
#   https://doc.qt.io/qt-6/qml-qtquick-row.html
#   https://doc.qt.io/qt-6/qml-qtquick-grid.html
#   https://doc.qt.io/qt-6/qml-qtquick-flow.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A positioner whose child is anchored where it places it
  A positioner places its children along its axes. A child anchored along one of them makes the
  positioner give up, and it says so at the place the positioner was declared.

  Scenario Outline: A <type> says so at its own declaration
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 200
        <type> { width: 100; height: 100
          Rectangle { width: 10; height: 10; anchors.<anchor>: parent.<anchor> } }
      }
      """
    Then the log has a warning matching "^.*[.]qml:4:9: QML <type>: Cannot specify <what> for items inside <type>. <type> will not function.$"

    Examples:
      | type   | anchor | what                                                    |
      | Column | top    | top, bottom, verticalCenter, fill or centerIn anchors   |
      | Row    | left   | left, right, horizontalCenter, fill or centerIn anchors |
      | Grid   | top    | anchors                                                 |
      | Flow   | left   | anchors                                                 |

  Scenario: A positioner a component makes says so at its place in the component
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        Component { id: c; Column { Rectangle { width: 10; height: 10; anchors.centerIn: parent } } }
        function make() { c.createObject(root) }
      }
      """
    When the method "make" of "root" is called
    Then the log has a warning matching "^.*[.]qml:5:28: QML Column: Cannot specify .* for items inside Column. Column will not function.$"
