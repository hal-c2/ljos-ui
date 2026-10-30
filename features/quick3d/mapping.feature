# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-camera.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Mapping between the view, the scene and a node
  A View3D maps a scene position to a point of the view and back; its z
  is the distance from the camera's near clip plane. A camera does the
  same in viewport coordinates, 0 to 1 from the top left. A node maps
  positions and directions between its own space and the scene, says
  where it is, how it is turned and scaled in the scene, which way its
  forward, up and right point, and can be turned to look at a point.

  # A 200 by 100 view; the camera 300 units out on +Z, its near plane 10
  # units in front of it, with a 60° vertical field of view. The node
  # "arm" stands at x 100, turned 90° about Y, so its +Z points along the
  # scene's +X; "hand" is 50 units out along it, twice the size.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        id: view; objectName: "view"; width: 200; height: 100
        property vector3d out
        function from3D(x, y, z) { out = mapFrom3DScene(Qt.vector3d(x, y, z)) }
        function to3D(x, y, z) { out = mapTo3DScene(Qt.vector3d(x, y, z)) }
        function toViewport(x, y, z) { out = cam.mapToViewport(Qt.vector3d(x, y, z)) }
        function fromViewport(x, y, z) { out = cam.mapFromViewport(Qt.vector3d(x, y, z)) }
        function toScene(x, y, z) { out = hand.mapPositionToScene(Qt.vector3d(x, y, z)) }
        function directionToScene(x, y, z) { out = hand.mapDirectionToScene(Qt.vector3d(x, y, z)) }
        function look(x, y, z) { cam.lookAt(Qt.vector3d(x, y, z)) }
        PerspectiveCamera { id: cam; objectName: "cam"; z: 300 }
        Node {
          objectName: "arm"; x: 100; eulerRotation.y: 90
          Node { id: hand; objectName: "hand"; z: 50; scale: Qt.vector3d(2, 2, 2) }
        }
      }
      """

  Rule: A View3D maps between the scene and the view

    Scenario: The scene's origin is in the middle of the view, 290 units past the near plane
      When the method "from3D" of "view" is called with 0, 0, 0
      Then the property "out.x" of "view" is within 0.01 of 100
      And the property "out.y" of "view" is within 0.01 of 50
      And the property "out.z" of "view" is within 0.01 of 290

    # 100 units up at 300 units away, with 173.2 units from the middle to
    # the top of the view: 28.87 of its 50 pixels up.
    Scenario: A point above the origin maps above the middle
      When the method "from3D" of "view" is called with 0, 100, 0
      Then the property "out.y" of "view" is within 0.01 of 21.13

    Scenario: A point behind the camera has a negative z
      When the method "from3D" of "view" is called with 0, 0, 400
      Then the property "out.z" of "view" is within 0.01 of -110

    Scenario: A point of the view maps back into the scene
      When the method "to3D" of "view" is called with 100, 50, 290
      Then the property "out.x" of "view" is within 0.01 of 0
      And the property "out.y" of "view" is within 0.01 of 0
      And the property "out.z" of "view" is within 0.01 of 0

  Rule: A camera maps between the scene and its viewport

    Scenario: The origin is in the middle of the viewport
      When the method "toViewport" of "view" is called with 0, 0, 0
      Then the property "out.x" of "view" is within 0.001 of 0.5
      And the property "out.y" of "view" is within 0.001 of 0.5
      And the property "out.z" of "view" is within 0.01 of 290

    Scenario: The middle of the viewport, 90 units past the near plane
      When the method "fromViewport" of "view" is called with 0.5, 0.5, 90
      Then the property "out.z" of "view" is within 0.01 of 200
      And the property "out.x" of "view" is within 0.01 of 0

    Scenario: A camera turned to look at a point faces it
      When the method "look" of "view" is called with 300, 0, 300
      Then the property "forward.x" of "cam" is within 0.001 of 1
      And the property "eulerRotation.y" of "cam" is within 0.01 of -90

  Rule: A node knows where it is in the scene

    Scenario: A child node's place, scale and facing in the scene
      Then the property "scenePosition.x" of "hand" is within 0.01 of 150
      And the property "scenePosition.z" of "hand" is within 0.01 of 0
      And the property "sceneScale.x" of "hand" is within 0.001 of 2
      And the property "forward.x" of "hand" is within 0.001 of -1
      And the property "right.z" of "hand" is within 0.001 of -1
      And the property "up.y" of "hand" is within 0.001 of 1

    Scenario: A scene property follows the node's parent
      When the property "x" of "arm" is set to 0
      Then the property "scenePosition.x" of "hand" is within 0.01 of 50

    Scenario: A position maps through the node's scale, turn and place
      When the method "toScene" of "view" is called with 0, 0, 10
      Then the property "out.x" of "view" is within 0.01 of 170
      And the property "out.z" of "view" is within 0.01 of 0

    Scenario: A direction maps through the turn alone
      When the method "directionToScene" of "view" is called with 0, 0, 1
      Then the property "out.x" of "view" is within 0.001 of 1
      And the property "out.z" of "view" is within 0.001 of 0
