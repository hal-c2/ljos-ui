# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-particles3d-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particlesystem3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particleemitter3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-spriteparticle3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-modelparticle3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-gravity3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-emitburst3d.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: Particles
  A system runs emitters that make particles of a kind and affectors that
  change what happens to them. A kind shows as sprites or as models. A system
  that is not running leaves nothing scheduled.

  # An orthographic camera, so one unit is one pixel, over a black window of
  # 200 by 200. A sprite is a square whose side is its particleScale.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          ParticleSystem3D {
            id: system; objectName: "system"; useRandomSeed: false
            SpriteParticle3D {
              id: grey; objectName: "kind"
              color: "#808080"; particleScale: 40
              fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone
            }
            ParticleEmitter3D {
              objectName: "emitter"; particle: grey
              lifeSpan: 100000
              velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }
              EmitBurst3D { amount: 1 }
            }
          }
        }
      }
      """

  Scenario: A sprite is a square of its size at the emitter, in its colour
    When 100 milliseconds pass
    Then the pixel at 100, 100 of the window is within 3 of "#808080"
    And the pixel at 85, 85 of the window is within 3 of "#808080"
    And the pixel at 70, 100 of the window is within 3 of "#000000"
    And the pixel at 130, 100 of the window is within 3 of "#000000"

  Scenario: A system not running schedules nothing
    When the property "running" of "system" is set to false
    And 100 milliseconds pass
    Then the application has nothing scheduled

  Scenario: A paused system schedules nothing and keeps what it shows
    When 100 milliseconds pass
    And the property "paused" of "system" is set to true
    And 100 milliseconds pass
    Then the application has nothing scheduled
    And the pixel at 100, 100 of the window is within 3 of "#808080"

  Scenario: A burst comes once
    When 100 milliseconds pass
    And the property "enabled" of "emitter" is set to false
    And 100 milliseconds pass
    Then the pixel at 100, 100 of the window is within 3 of "#808080"
