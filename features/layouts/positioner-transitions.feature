# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-column.html#populate-prop
#   https://doc.qt.io/qt-6/qml-qtquick-column.html#add-prop
#   https://doc.qt.io/qt-6/qml-qtquick-column.html#move-prop
#   https://doc.qt.io/qt-6/qtquick-positioning-layouts.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Positioner transitions and anchored children
  A positioner runs populate when it first places its children and move when
  a child changes place. A child anchored along the positioner's axis makes
  it give up, with a warning.

  Scenario: A Column with an anchored child does not function
    Given the QML:
      """qml
      import QtQuick
      Column {
        objectName: "col"
        Rectangle { objectName: "a"; width: 10; height: 10 }
        Rectangle { objectName: "b"; width: 10; height: 10; anchors.top: parent.top }
      }
      """
    Then the log has a warning matching "Cannot specify top, bottom, verticalCenter, fill or centerIn anchors for items inside Column"
    And the property "y" of "a" is 0

  Scenario: A child takes the place of one that went away, with a move transition
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Column {
          Rectangle { id: a; objectName: "a"; width: 10; height: 10 }
          Rectangle { id: b; objectName: "b"; width: 10; height: 10 }
          move: Transition { PropertyAnimation { properties: "y"; duration: 50 } }
        }
        Timer { interval: 10; running: true; onTriggered: a.visible = false }
      }
      """
    When 200 milliseconds pass
    Then the property "y" of "b" is 0
