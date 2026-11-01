# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A PrincipledMaterial's reflection of the lights
  How much a dielectric reflects, and what, follows its specular amount,
  tint and index of refraction and its Fresnel settings; a metal keeps
  all its reflection. These scenes look straight at a black square lit
  head on, so all that is seen is the reflection, and the values follow
  from the documented model at roughness 0.5.

  Rule: The specular amount scales a dielectric's reflection

    Scenario: All of it
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5;  } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#707070"

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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; specularAmount: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#505050"

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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; specularAmount: 0 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#000000"

    Scenario: A metal keeps its reflection
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "#404040"; metalness: 1; roughness: 0.5; specularAmount: 0 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#7d7d7d"

  Rule: The specular tint takes the reflection towards the base colour

    Scenario: Untinted, a red surface reflects white
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "red"; roughness: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#ff7070"

    Scenario: Tinted, it reflects red
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "red"; roughness: 0.5; specularTint: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#ff0000"

  Rule: The index of refraction sets what is reflected head on

    Scenario: A denser surface reflects more
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; indexOfRefraction: 2 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#b2b2b2"

  Rule: Fresnel settings change the reflection when they are enabled

    Scenario: The bias adds to it
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; fresnelScaleBiasEnabled: true; fresnelBias: 0.05 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#a0a0a0"

    Scenario: Not enabled, the bias does nothing
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; fresnelBias: 0.05 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#707070"

    Scenario: A lower power brightens what is seen at an angle
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
            DirectionalLight { eulerRotation.y: 90 }
            Model { eulerRotation.y: 45; source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; fresnelPower: 2 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 8 of "#c4c4c4"

    Scenario: At the usual power it is dimmer
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
            DirectionalLight { eulerRotation.y: 90 }
            Model { eulerRotation.y: 45; source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5;  } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 8 of "#787878"

  Rule: A surface's height map moves what its other maps show

    Scenario: Seen at an angle, a surface below its height shows what lies further along
      Given a 64 by 64 image file "quadrants.png" of quadrants "#ff0000, #00ff00, #0000ff, #ffffff"
      And a 64 by 64 image file "black.png" of quadrants "#000000, #000000, #000000, #000000"
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
            Model { eulerRotation.y: 45; source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; heightAmount: 0.2; baseColorMap: Texture { source: "quadrants.png" }
          heightMap: Texture { source: "black.png" } } }
          }
        }
        """
      Then the pixel at 95, 75 of the window is within 2 of "#00ff00"

    Scenario: Without an amount it stays
      Given a 64 by 64 image file "quadrants.png" of quadrants "#ff0000, #00ff00, #0000ff, #ffffff"
      And a 64 by 64 image file "black.png" of quadrants "#000000, #000000, #000000, #000000"
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
            Model { eulerRotation.y: 45; source: "#Rectangle"; materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColorMap: Texture { source: "quadrants.png" }
          heightMap: Texture { source: "black.png" } } }
          }
        }
        """
      Then the pixel at 95, 75 of the window is within 2 of "#ff0000"

  Rule: A material can be lit by a light probe of its own

    Scenario: It is lit where nothing else lights it
      Given a 64 by 64 image file "white.png" of quadrants "#ffffff, #ffffff, #ffffff, #ffffff"
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1; lightProbe: Texture { source: "white.png" } } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 12 of "#ffffff"

    Scenario: Without it, it is dark
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { specularAmount: 0; roughness: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#000000"

  Rule: Specular antialiasing widens a highlight too small for its pixels

    Scenario: On a small sphere it is brighter around the middle
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color; specularAAEnabled: true }
            OrthographicCamera { z: 600 }
            DirectionalLight { }
            Model { scale: Qt.vector3d(0.2, 0.2, 0.2); source: "#Sphere"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#b2b2b2"

    Scenario: Without it the highlight falls between the pixels
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
            Model { scale: Qt.vector3d(0.2, 0.2, 0.2); source: "#Sphere"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#373737"
