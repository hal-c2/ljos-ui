# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-principledmaterial.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A clear coat over a PrincipledMaterial
  The coat reflects as a dielectric of its own, with its own roughness
  and Fresnel settings, over what is under it. These surfaces reflect
  nothing themselves, so all that is seen head on under one light is the
  coat.

  Rule: The clearcoat amount scales the coat

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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; specularAmount: 0; clearcoatAmount: 1; clearcoatRoughnessAmount: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#707070"

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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; specularAmount: 0; clearcoatAmount: 0.5; clearcoatRoughnessAmount: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#505050"

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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; specularAmount: 0; clearcoatRoughnessAmount: 0.5 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 4 of "#000000"

  Rule: The coat has its own roughness and Fresnel settings

    Scenario: A rough coat spreads its light
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; specularAmount: 0; clearcoatAmount: 1; clearcoatRoughnessAmount: 1 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 8 of "#191919"

    Scenario: Its bias adds to it
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
            Model { source: "#Rectangle"; materials: PrincipledMaterial { baseColor: "black"; roughness: 0.5; specularAmount: 0; clearcoatAmount: 1; clearcoatRoughnessAmount: 0.5; clearcoatFresnelScaleBiasEnabled: true; clearcoatFresnelBias: 0.05 } }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 6 of "#a0a0a0"
