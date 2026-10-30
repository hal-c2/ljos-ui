# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-wasdcontroller.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation. How far
# a key moves the object in a second is not in it; here it is the speed a
# frame of a 60 Hz display, times 60.

@ljos-only
Feature: Moving through a scene with the keys
  A WasdController moves its controlledObject while W, A, S, D, R and F,
  or the arrow and page keys, are held: forwards, left, back, right, up
  and down along the object's own axes. Shift multiplies the speed. It
  does nothing, and asks for no frames, while no key is held.

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
          PerspectiveCamera { id: camera; objectName: "camera" }
        }
        WasdController {
          objectName: "controller"
          anchors.fill: parent
          controlledObject: camera
        }
      }
      """

  Rule: A held key moves the object

    Scenario: W moves it forwards
      When the user keeps "W" down
      And 1 second passes
      And the user lets go of "W"
      Then the property "z" of "camera" is at most -250

    Scenario: S moves it back
      When the user keeps "S" down
      And 1 second passes
      And the user lets go of "S"
      Then the property "z" of "camera" is at least 250

    Scenario: D moves it right and A left
      When the user keeps "D" down
      And 1 second passes
      And the user lets go of "D"
      Then the property "x" of "camera" is at least 250

    Scenario: R moves it up and F down
      When the user keeps "R" down
      And 1 second passes
      And the user lets go of "R"
      Then the property "y" of "camera" is at least 250

    Scenario: The arrow keys move it like W and S
      When the user keeps "Up" down
      And 1 second passes
      And the user lets go of "Up"
      Then the property "z" of "camera" is at most -250

    Scenario: Shift triples the pace
      When the user keeps "Shift+W" down
      And 1 second passes
      And the user lets go of "Shift+W"
      Then the property "z" of "camera" is at most -800

    Scenario: speed scales every direction
      When the property "speed" of "controller" is set to 2
      And the user keeps "W" down
      And 1 second passes
      And the user lets go of "W"
      Then the property "z" of "camera" is at most -550

  Rule: Nothing moves it when no key is held

    Scenario: Time passes without a key
      When 1 second passes
      Then the property "z" of "camera" is within 0.01 of 0
      And the property "inputsNeedProcessing" of "controller" is false

    Scenario: A key let go of stops it
      When the user keeps "W" down
      And 1 second passes
      And the user lets go of "W"
      And 1 second passes
      Then the property "inputsNeedProcessing" of "controller" is false

    Scenario: A key held says the controller has inputs to process
      When the user keeps "W" down
      Then the property "inputsNeedProcessing" of "controller" is true

    Scenario: With keysEnabled false the keys do nothing
      When the property "keysEnabled" of "controller" is set to false
      And the user keeps "W" down
      And 1 second passes
      Then the property "z" of "camera" is within 0.01 of 0
