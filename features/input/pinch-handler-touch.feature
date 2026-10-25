# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pinchhandler.html
#   https://doc.qt.io/qt-6/qtquick-input-handlers.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PinchHandler.persistentScale @api-QtQuick.PinchHandler.persistentRotation
Feature: Two fingers on a PinchHandler
  A PinchHandler takes two fingers once one of them has gone the drag
  threshold. It scales and turns its target from there, and what the first
  finger pressed is cancelled.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 300
        property string log: ""
        MouseArea {
          anchors.fill: parent
          onPressed: parent.log += "P "
          onCanceled: parent.log += "C "
        }
        Rectangle {
          objectName: "box"
          x: 50; y: 50; width: 200; height: 200
          PinchHandler { objectName: "pinch" }
        }
      }
      """

  Scenario: Spreading two fingers scales from where the pinch took over
    When the user pinches at 150, 150 from 100 to 300 pixels apart
    Then the property "scale" of "box" is within 0.01 of 2.419
    And the property "persistentScale" of "pinch" is within 0.01 of 2.419
    And the property "active" of "pinch" is false

  Scenario: Turning two fingers rotates clockwise from where the pinch took over
    When the user turns two fingers 100 pixels apart at 150, 150 by 90 degrees
    Then the property "rotation" of "box" is within 0.5 of 75
    And the property "persistentRotation" of "pinch" is within 0.5 of 75

  Scenario: The pinch cancels what the first finger pressed
    When the user pinches at 150, 150 from 100 to 300 pixels apart
    Then the property "log" of "root" is "P C "
