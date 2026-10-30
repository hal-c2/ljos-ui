# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-object3d.html#parent-prop
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html#scenePosition-prop
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

Feature: A node's parent
  Object3D.parent is the object a node is in: the one it is declared in,
  or the one its parent is set to. A node is placed through its parent's
  transform, so its scene properties, and where it is drawn, follow its
  parent, a declared one or one it is moved to later.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        objectName: "root"; width: 100; height: 100
        environment: SceneEnvironment { backgroundMode: SceneEnvironment.Color; clearColor: "black" }
        PerspectiveCamera { z: 300 }
        Node { id: away; x: 1000 }
        Node { id: here }
        Node { id: holder; x: 10; Node { id: inner; objectName: "inner"; x: 2 } }
        Node { id: moved; objectName: "moved"; parent: holder; x: 1 }
        Model {
          id: cube; objectName: "cube"; parent: away; source: "#Cube"
          materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" }
        }
        property real movedX: moved.scenePosition.x
        property real innerX: inner.scenePosition.x
        property real holderX: holder.scenePosition.x
        property bool innerInHolder: inner.parent === holder
        property bool movedInHolder: moved.parent === holder
        property bool movedHere: moved.parent === here
        function moveToHere() { moved.parent = here }
        function moveCubeHere() { cube.parent = here }
        function moveHolderIntoInner() { holder.parent = inner }
      }
      """

  Scenario: A node's parent is the node it is declared in
    Then the property "innerInHolder" of "root" is true
    And the property "innerX" of "root" is 12

  Scenario: A declared parent places the node through its transform
    Then the property "movedInHolder" of "root" is true
    And the property "movedX" of "root" is 11

  Scenario: Moving a node to another parent moves its scene position
    When the method "moveToHere" of "root" is called
    Then the property "movedHere" of "root" is true
    And the property "movedInHolder" of "root" is false
    And the property "movedX" of "root" is 1

  Scenario: Moving a parent moves what was moved into it
    When the method "moveToHere" of "root" is called
    And the property "x" of "moved" is set to 5
    Then the property "movedX" of "root" is 5

  Scenario: A model is drawn where its parent puts it
    Then the pixel at 50, 50 of the window is within 2 of "#000000"
    When the method "moveCubeHere" of "root" is called
    Then the pixel at 50, 50 of the window is within 2 of "#ff0000"

  Scenario: A node cannot be put inside what it holds
    When the method "moveHolderIntoInner" of "root" is called
    Then the property "innerInHolder" of "root" is true
    And the property "holderX" of "root" is 10
    And the property "innerX" of "root" is 12
