# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-orbitcameracontroller.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation. The
# direction of a drag is the documentation's default of yInvert: true.

@ljos-only
Feature: Orbiting a camera with the pointer
  An OrbitCameraController turns its origin node when the pointer drags
  over it, moves its camera towards or away from the origin when the wheel
  turns, and moves the origin across the view when a drag is made with
  Ctrl held.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Item {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          Node {
            id: origin; objectName: "origin"
            PerspectiveCamera { id: camera; objectName: "camera"; z: 300 }
          }
        }
        OrbitCameraController {
          objectName: "controller"
          anchors.fill: parent
          origin: origin; camera: camera
        }
      }
      """

  Rule: A drag turns the origin

    Scenario: A drag to the left turns the origin the other way
      When the user drags from 100, 100 to 50, 100
      Then the property "eulerRotation.y" of "origin" is within 0.5 of 5

    Scenario: A drag down tilts the origin
      When the user drags from 100, 100 to 100, 150
      Then the property "eulerRotation.x" of "origin" is within 0.5 of 5

    Scenario: xSpeed is how far a pixel turns it
      When the property "xSpeed" of "controller" is set to 0.2
      And the user drags from 100, 100 to 50, 100
      Then the property "eulerRotation.y" of "origin" is within 0.5 of 10

    Scenario: xInvert turns it the other way
      When the property "xInvert" of "controller" is set to true
      And the user drags from 100, 100 to 50, 100
      Then the property "eulerRotation.y" of "origin" is within 0.5 of -5

    Scenario: A drag does nothing when the mouse is not enabled
      When the property "mouseEnabled" of "controller" is set to false
      And the user drags from 100, 100 to 50, 100
      Then the property "eulerRotation.y" of "origin" is within 0.01 of 0

  Rule: The wheel moves the camera

    Scenario: Turning the wheel up brings the camera closer
      When the user turns the wheel 1 notch up at 100, 100
      Then the property "z" of "camera" is at most 280

    Scenario: Turning the wheel down takes it away
      When the user turns the wheel 1 notch down at 100, 100
      Then the property "z" of "camera" is at least 320

    Scenario: The camera never reaches the origin
      When the user turns the wheel 100 notches up at 100, 100
      Then the property "z" of "camera" is at least 1

  Rule: Ctrl and a drag moves the origin

    Scenario: A drag with Ctrl held moves the origin and does not turn it
      When the user drags from 100, 100 to 150, 100 holding "Ctrl"
      Then the property "eulerRotation.y" of "origin" is within 0.01 of 0
      And the property "x" of "origin" is at most -10

    Scenario: panEnabled false stops it
      When the property "panEnabled" of "controller" is set to false
      And the user drags from 100, 100 to 150, 100 holding "Ctrl"
      Then the property "x" of "origin" is within 0.01 of 0
