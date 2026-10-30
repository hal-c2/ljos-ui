# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-lookatnode.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A node that looks at another
  A LookAtNode turns about X and Y so that its -Z points at its target,
  and keeps doing so when either node moves. It never rolls.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Helpers
      Item {
        objectName: "root"
        width: 100; height: 100
        property real yaw: look.eulerRotation.y
        property real pitch: look.eulerRotation.x
        property real roll: look.eulerRotation.z
        View3D {
          anchors.fill: parent
          Node { id: goal; objectName: "goal"; x: 100 }
          LookAtNode { id: look; target: goal }
          PerspectiveCamera { z: 300 }
        }
      }
      """

  Scenario: A target to the right is looked at by turning to the right
    Then the property "yaw" of "root" is within 0.01 of -90
    And the property "pitch" of "root" is within 0.01 of 0
    And the property "roll" of "root" is within 0.01 of 0

  Scenario: A target above is looked at by turning up
    When the property "x" of "goal" is set to 0
    And the property "y" of "goal" is set to 100
    Then the property "pitch" of "root" is within 0.01 of 90

  Scenario: A target that moves is followed
    When the property "z" of "goal" is set to -100
    Then the property "yaw" of "root" is within 0.01 of -45

  Scenario: The node that moves is followed too
    When the property "x" of "goal" is set to 0
    And the property "z" of "goal" is set to -100
    Then the property "yaw" of "root" is within 0.01 of 0
