# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-defaultmaterial.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A DefaultMaterial's opacity, reflection and glow
  A DefaultMaterial has its own opacity, a specular reflection with its
  own amount, tint, roughness and map, and an emissive colour with a
  map.

  Rule: Opacity lets what is behind show

    Scenario: Half opaque
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
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red"; opacity: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#800080"

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
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red"; opacityChannel: Material.R; opacityMap: Texture { source: "quadrants.png" } } }
          }
        }
        """
      Then the pixel at 75, 75 of the window is within 4 of "#ff0000"
      And the pixel at 125, 75 of the window is within 4 of "#0000ff"

  Rule: The specular reflection has an amount, a tint and a map

    Scenario: Its amount
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
            Model { source: "#Rectangle"; materials: DefaultMaterial { diffuseColor: "black"; specularAmount: 0.25; specularRoughness: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 8 of "#ffffff"

    Scenario: Its tint
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
            Model { source: "#Rectangle"; materials: DefaultMaterial { diffuseColor: "black"; specularAmount: 0.25; specularRoughness: 0.5; specularTint: "red" } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 8 of "#ff0000"

    Scenario: Its map
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
            Model { source: "#Rectangle"; materials: DefaultMaterial { diffuseColor: "black"; specularAmount: 0.25; specularRoughness: 0.5; specularMap: Texture { source: "quadrants.png" } } }
          }
        }
        """
      Then the pixel at 75, 75 of the window is within 8 of "#ff0000"
      And the pixel at 125, 75 of the window is within 8 of "#00ff00"

  Rule: It glows by its emissive colour and map

    Scenario: The colour
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
            Model { source: "#Rectangle"; materials: DefaultMaterial { diffuseColor: "black"; emissiveFactor: Qt.vector3d(1, 0, 0) } }
            DirectionalLight { brightness: 0 }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#ff0000"

    Scenario: The map
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
            Model { source: "#Rectangle"; materials: DefaultMaterial { diffuseColor: "black"; emissiveFactor: Qt.vector3d(1, 1, 1); emissiveMap: Texture { source: "quadrants.png" } } }
            DirectionalLight { brightness: 0 }
          }
        }
        """
      Then the pixel at 125, 75 of the window is within 4 of "#00ff00"

  Rule: A blend mode lays it over what is behind

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
            Model { source: "#Rectangle"; z: -50; scale: Qt.vector3d(4, 4, 1); materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: "blue" } }
            Model { source: "#Rectangle"; materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red"; blendMode: DefaultMaterial.Screen } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#ff00ff"
