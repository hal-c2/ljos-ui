# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html#scene-prop
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: The scene of a View3D
  A View3D's scene is the root node of what it shows. Nodes made into it at
  run time are drawn with those declared in the view.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        objectName: "view"
        width: 100; height: 100
        environment: SceneEnvironment {
          backgroundMode: SceneEnvironment.Color
          clearColor: "black"
        }
        PerspectiveCamera { z: 300 }
        function add() {
          Qt.createQmlObject('import QtQuick3D; Model { source: "#Cube"; '
            + 'materials: PrincipledMaterial { '
            + 'lighting: PrincipledMaterial.NoLighting; baseColor: "white" } }',
            scene)
        }
      }
      """

  Scenario: A scene nothing was made into draws nothing
    When 100 milliseconds pass
    Then the pixel at 50, 50 of the window is "#000000"

  Scenario: A node made into the scene is drawn
    When the method "add" of "view" is called
    And 100 milliseconds pass
    Then the pixel at 50, 50 of the window is "#ffffff"
