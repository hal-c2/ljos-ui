# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pincharea.html
#   https://doc.qt.io/qt-6/qml-qtquick-pinchevent.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.PinchArea.pinch @api-QtQuick.PinchArea.pinchStarted @api-QtQuick.PinchArea.pinchUpdated @api-QtQuick.PinchArea.pinchFinished
Feature: PinchArea
  A PinchArea starts a pinch once two fingers on it are 10 pixels nearer
  or further apart than when the second came down. It scales its
  pinch.target within pinch.minimumScale and pinch.maximumScale, which
  are both 1 unless set.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 300
        property string log: ""
        property int updates: 0
        Rectangle { objectName: "box"; x: 100; y: 100; width: 100; height: 100 }
        PinchArea {
          objectName: "area"
          anchors.fill: parent
          pinch.target: parent.children[0]
          property real most: 1
          pinch.maximumScale: most
          property bool on: pinch.active
          onPinchStarted: (p) => parent.log += "S" + on + p.pointCount + " "
          onPinchUpdated: (p) => parent.updates++
          onPinchFinished: (p) => parent.log += "F" + p.pointCount + on
        }
      }
      """

  Scenario: A pinch starts, updates and finishes
    When the user pinches at 150, 150 from 100 to 200 pixels apart
    Then the property "log" of "root" is "Sfalse2 F0true"
    And the property "updates" of "root" is 11
    And the property "pinch.active" of "area" is false

  Scenario: The target keeps its scale while the scale limits are both 1
    When the user pinches at 150, 150 from 100 to 200 pixels apart
    Then the property "scale" of "box" is 1

  Scenario: The target scales from where the pinch started
    When the property "most" of "area" is set to 10
    And the user pinches at 150, 150 from 100 to 200 pixels apart
    Then the property "scale" of "box" is within 0.01 of 1.724
