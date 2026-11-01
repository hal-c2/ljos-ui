# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-light.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A light's scope
  A light with a scope lights only that node and what is inside it;
  without one it lights everything.

  Rule: A scope limits what a light lights

    Scenario: Inside and outside
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
            Node { id: inside; Model { x: -50; scale: Qt.vector3d(0.5, 0.5, 1); source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } } }
            Model { x: 50; scale: Qt.vector3d(0.5, 0.5, 1); source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
            DirectionalLight { scope: inside }
          }
        }
        """
      Then the pixel at 50, 100 of the window is within 4 of "#ffffff"
      And the pixel at 150, 100 of the window is within 4 of "#000000"

    Scenario: No scope
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
            Node { id: inside; Model { x: -50; scale: Qt.vector3d(0.5, 0.5, 1); source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } } }
            Model { x: 50; scale: Qt.vector3d(0.5, 0.5, 1); source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
            DirectionalLight { }
          }
        }
        """
      Then the pixel at 50, 100 of the window is within 4 of "#ffffff"
      And the pixel at 150, 100 of the window is within 4 of "#ffffff"

    Scenario: Its ambient too
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
            Node { id: inside; Model { x: -50; scale: Qt.vector3d(0.5, 0.5, 1); source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } } }
            Model { x: 50; scale: Qt.vector3d(0.5, 0.5, 1); source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
            DirectionalLight { scope: inside; brightness: 0; ambientColor: "white" }
          }
        }
        """
      Then the pixel at 50, 100 of the window is within 8 of "#ffffff"
      And the pixel at 150, 100 of the window is within 4 of "#000000"
