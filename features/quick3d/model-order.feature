# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: What decides which of a scene's surfaces shows
  A model's depth bias is added to its distance from the camera when
  transparent surfaces are put in order, and an environment can turn
  depth testing off, so that a surface draws over whatever came before
  it.

  Rule: The depth bias changes the order of transparent surfaces

    Scenario: The nearer one is on top
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#000000"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red"; opacity: 0.5 } }
            Model { source: "#Rectangle"; z: 1; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "lime"; opacity: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 8 of "#408000"

    Scenario: Pushed back, it goes under
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#000000"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red"; opacity: 0.5 } }
            Model { source: "#Rectangle"; z: 1; depthBias: 10; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "lime"; opacity: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 8 of "#804000"

  Rule: Depth testing can be off

    Scenario: On, the nearer one shows
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; z: 10; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red" } }
            Model { source: "#Rectangle"; z: 0; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"

    Scenario: Off, the farther one is drawn over it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; depthTestEnabled: false }
            OrthographicCamera { z: 600 }
            Model { source: "#Rectangle"; z: 10; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red" } }
            Model { source: "#Rectangle"; z: 0; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#0000ff"
