# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

Feature: A shaded material lights itself from the probe
  A material's IBL_PROBE takes the place of what the light probe would have
  done. It reads the probe through IBL_TEXTURE, turned by IBL_ORIENTATION, and
  is told IBL_EXPOSE and IBL_MAXMIPMAP.

  # The probe is 0.5 above the horizon and 0 below it. A square facing the
  # camera shows what the probe has in a direction. Linear 0.5 is #bcbcbc.
  Background:
    Given a 64 by 32 Radiance image file "halves.hdr" with an upper half of 0.5, 0.5, 0.5 and a lower half of 0, 0, 0
    And the text file "probe.frag":
      """
      void IBL_PROBE()
      {
          vec3 up = IBL_ORIENTATION * vec3(0.0, 1.0, 0.0);
          // The sharpest level, so that down finds none of the upper half.
          DIFFUSE += textureLod(IBL_TEXTURE, up, 0.0).rgb * IBL_EXPOSE * step(1.0, IBL_MAXMIPMAP);
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 100; height: 100; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment {
            objectName: "env"; backgroundMode: SceneEnvironment.Color; clearColor: "black"
            lightProbe: Texture { source: "halves.hdr" }
          }
          OrthographicCamera { z: 600 }
          Model {
            source: "#Rectangle"; scale: Qt.vector3d(2, 2, 2)
            materials: CustomMaterial {
              shadingMode: CustomMaterial.Shaded
              fragmentShader: "probe.frag"
            }
          }
        }
      }
      """

  Scenario: Up in the probe is bright
    Then the pixel at 50, 50 of the window is within 3 of "#bcbcbc"
    And the log has no warnings

  Scenario: The orientation turns what a direction finds
    When the property "probeOrientation.x" of "env" is set to 180
    Then the pixel at 50, 50 of the window is within 3 of "#000000"

  Scenario: Exposure is told to the material
    When the property "probeExposure" of "env" is set to 0
    Then the pixel at 50, 50 of the window is within 3 of "#000000"
