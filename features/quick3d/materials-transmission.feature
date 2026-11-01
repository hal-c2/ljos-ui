# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: What shows through a material, and how it is laid over it
  Transmission lets what is behind take the place of a surface's diffuse
  light; its reflections stay. A blend mode lays a surface over what is
  behind by other rules than simply covering it. Refraction and tinting
  of what is behind are not modelled.

  Rule: Transmission gives diffuse light over to what is behind

    Scenario: Half of it
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
            DirectionalLight { }
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "red"; roughness: 1; specularAmount: 0; transmissionFactor: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#bc0080"

    Scenario: None of it
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
            DirectionalLight { }
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "red"; roughness: 1; specularAmount: 0 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#ff0000"

    Scenario: A map's channel multiplies it
      Given a 64 by 64 image file "quadrants.png" of quadrants "#ff0000, #00ff00, #0000ff, #ffffff"
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "red"; roughness: 1; specularAmount: 0; transmissionFactor: 1; transmissionChannel: Material.R; transmissionMap: Texture { source: "quadrants.png" } } }
          }
        }
        """
      Then the pixel at 75, 75 of the window is within 6 of "#0000ff"
      And the pixel at 125, 75 of the window is within 4 of "#ff0000"

  Rule: A blend mode changes how a surface covers what is behind

    Scenario: Screen adds light
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
            DirectionalLight { }
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red"; blendMode: PrincipledMaterial.Screen } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#ff00ff"

    Scenario: Multiply keeps what both have
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
            DirectionalLight { }
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red"; blendMode: PrincipledMaterial.Multiply } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#000000"

    Scenario: Multiply over white shows the surface
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
            DirectionalLight { }
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "white" } }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red"; blendMode: PrincipledMaterial.Multiply } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"

    Scenario: The usual mode covers
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
            DirectionalLight { }
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "red" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#ff0000"
