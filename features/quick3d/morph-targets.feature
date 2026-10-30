# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-runtimeloader.html
#   https://doc.qt.io/qt-6/quick3d-morphing.html
#   https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A loaded mesh drawn with its morph targets' weights
  A glTF mesh may carry morph targets: other positions, normals and
  tangents for its vertices. Each vertex is drawn at its own position plus
  each target's displacement times the target's weight, the weights being
  the node's, or else the mesh's. The documentation promises nothing about
  playing a file's animations, so a RuntimeLoader draws the weights the
  file gives.

  # morph.gltf, made by features/quick3d/models/gen.py, is a square 100
  # units across, unlit red, with one target that moves every vertex 100
  # units right, and a weight of 0.5: the square is drawn 50 units right.
  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin.
  Rule: A target moves the vertices by its weight

    Scenario: The square is drawn halfway to its target
      Given the model file "morph.gltf"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            RuntimeLoader { objectName: "loader"; source: "morph.gltf" }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "loader" is 1
      And the pixel at 190, 100 of the window is within 2 of "#ff0000"
      And the pixel at 60, 100 of the window is "#202020"
      And the property "bounds.maximum.x" of "loader" is within 0.001 of 100
