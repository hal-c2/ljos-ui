# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flipable.html
#   https://doc.qt.io/qt-6/qml-qtquick-rotation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Flipable
  A Flipable has a front and a back. Which one shows depends on whether its own transform turns it
  over; the back is mirrored so that it reads the right way round once turned.

  Scenario: A Flipable that is not turned shows its front
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 100; height: 100; color: "black"
        Flipable {
          objectName: "card"
          width: 100; height: 100
          front: Rectangle { width: 100; height: 100; color: "#ff0000" }
          back: Rectangle { width: 100; height: 100; color: "#00ff00" }
        }
      }
      """
    Then the property "side" of "card" is 0
    And the pixel at 50, 50 of the window is "#ff0000"

  Scenario: Turned half way round about the y axis it shows its back the right way round
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 100; height: 100; color: "black"
        Flipable {
          objectName: "card"
          width: 100; height: 100
          front: Rectangle { width: 100; height: 100; color: "#ff0000" }
          back: Row {
            Rectangle { width: 30; height: 100; color: "#0000ff" }
            Rectangle { width: 70; height: 100; color: "#00ff00" }
          }
          transform: Rotation { axis { x: 0; y: 1; z: 0 } origin.x: 50; origin.y: 50; angle: 180 }
        }
      }
      """
    Then the property "side" of "card" is 1
    And the pixel at 15, 50 of the window is "#0000ff"
    And the pixel at 80, 50 of the window is "#00ff00"

  Scenario: Turning it back shows the front again
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        width: 100; height: 100; color: "black"
        property alias angle: turn.angle
        Flipable {
          objectName: "card"
          width: 100; height: 100
          front: Rectangle { width: 100; height: 100; color: "#ff0000" }
          back: Rectangle { width: 100; height: 100; color: "#00ff00" }
          transform: Rotation { id: turn; axis { x: 0; y: 1; z: 0 } origin.x: 50; angle: 180 }
        }
      }
      """
    When the property "angle" of "root" is set to 0
    Then the property "side" of "card" is 0
    And the pixel at 50, 50 of the window is "#ff0000"

  Scenario Outline: What turns a Flipable over
    Given the QML:
      """qml
      import QtQuick
      Item {
        Flipable { objectName: "card"; width: 10; height: 10; <turn> }
      }
      """
    Then the property "side" of "card" is <side>

    Examples:
      | turn                                                             | side |
      | rotation: 180                                                    | 0    |
      | transform: Scale { xScale: -1 }                                  | 1    |
      | transform: Rotation { axis { x: 1; y: 0; z: 0 } angle: 100 }     | 1    |
      | transform: Rotation { axis { x: 0; y: 1; z: 0 } angle: 60 }      | 0    |

  Scenario: Only the Flipable's own transform counts
    Given the QML:
      """qml
      import QtQuick
      Item {
        transform: Scale { yScale: -1 }
        Flipable { objectName: "card"; width: 10; height: 10 }
      }
      """
    Then the property "side" of "card" is 0

  Scenario: The front and the back become its children
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool parents: card.front.parent === card && card.back.parent === card
        Flipable {
          id: card
          front: Item {}
          back: Item {}
        }
      }
      """
    Then the property "parents" of "root" is true

  Scenario: The keys of the side are numbers
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var keys: [Flipable.Front, Flipable.Back]
      }
      """
    Then the property "keys" of "root" is [0, 1]
