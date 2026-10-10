# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
#   https://doc.qt.io/qt-6/qml-qtquick-mouseevent.html
#   https://doc.qt.io/qt-6/qtquick-input-mouseevents.html
# The Qt oracle cannot inject pointer input, so these run on Ljós only.

@ljos-only
Feature: MouseArea
  A press goes to the topmost MouseArea under the pointer; rejecting it
  passes it to the next one down.

  Scenario: The topmost MouseArea gets the click
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property int under: 0
        property int over: 0
        MouseArea { width: 100; height: 100; onClicked: root.under++ }
        MouseArea { width: 50; height: 50; onClicked: root.over++ }
        id: root
      }
      """
    When the user touches 10, 10
    Then the property "over" of "root" is 1
    And the property "under" of "root" is 0

  Scenario: Rejecting the press passes it down
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property int under: 0
        MouseArea { width: 100; height: 100; onClicked: root.under++ }
        MouseArea {
          width: 100; height: 100
          onPressed: (mouse) => mouse.accepted = false
        }
        id: root
      }
      """
    When the user touches 10, 10
    Then the property "under" of "root" is 1

  Scenario: A disabled MouseArea lets the press through
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property int under: 0
        property int over: 0
        MouseArea { width: 100; height: 100; onClicked: root.under++ }
        MouseArea { width: 100; height: 100; enabled: false; onClicked: root.over++ }
        id: root
      }
      """
    When the user touches 10, 10
    Then the property "under" of "root" is 1
    And the property "over" of "root" is 0

  Scenario: The event position is in the MouseArea's coordinates
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property real seenX: -1
        property real seenY: -1
        MouseArea {
          x: 30; y: 40; width: 50; height: 50
          onClicked: (mouse) => { root.seenX = mouse.x; root.seenY = mouse.y }
        }
        id: root
      }
      """
    When the user touches 35, 47
    Then the property "seenX" of "root" is 5
    And the property "seenY" of "root" is 7
