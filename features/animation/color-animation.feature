# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-coloranimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-animationcontroller.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ColorAnimation
  A colour moves channel by channel, in whole steps of the 0 to 255 range.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Rectangle { id: box; objectName: "box"; color: "#101010" }
        Rectangle { id: lamp; objectName: "lamp"; color: "#101010" }
        ColorAnimation {
          id: fade; target: box; property: "color"; from: "#000000"; to: "#6496c8"; duration: 1000
        }
        PropertyAnimation {
          id: plain; target: box; property: "color"; from: "#000000"; to: "#ff8040"; duration: 1000
        }
        ColorAnimation { id: clear; target: box; property: "color"; from: "#00000000"; to: "#80ff0000" }
        ColorAnimation { objectName: "run"; target: lamp; property: "color"; to: "red"; duration: 100 }
        ColorAnimation { objectName: "bare"; target: lamp; to: "red"; duration: 100 }
        AnimationController { id: ctl; animation: fade }
        function show(animation, progress) {
          ctl.animation = [fade, plain, clear][animation]
          ctl.progress = progress
        }
      }
      """

  Scenario Outline: A ColorAnimation at <progress> of the way shows <color>
    When the method "show" of "root" is called with 0, <progress>
    Then the property "color" of "box" is "<color>"

    Examples:
      | progress | color   |
      | 0        | #000000 |
      | 0.25     | #192532 |
      | 0.5      | #324b64 |
      | 0.75     | #4b7096 |
      | 1        | #6496c8 |

  Scenario Outline: A PropertyAnimation on a colour at <progress> of the way shows <color>
    When the method "show" of "root" is called with 1, <progress>
    Then the property "color" of "box" is "<color>"

    Examples:
      | progress | color   |
      | 0.1      | #190c06 |
      | 0.5      | #7f4020 |
      | 0.9      | #e57339 |

  Scenario: The alpha channel is animated too
    When the method "show" of "root" is called with 2, 0.5
    Then the property "color" of "box" is "#407f0000"

  Scenario: A started ColorAnimation ends at its to colour
    When the method "start" of "run" is called
    And 1 second passes
    Then the property "color" of "lamp" is "#ff0000"

  Scenario: Without from, the colour starts where it is
    When the method "start" of "run" is called
    Then the property "color" of "lamp" is "#101010"

  Scenario: A ColorAnimation that names no property changes nothing
    When the method "start" of "bare" is called
    And 1 second passes
    Then the property "color" of "lamp" is "#101010"
    And the property "property" of "bare" is ""
