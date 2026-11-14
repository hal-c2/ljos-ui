# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particlesystem3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-node.html#visible-prop
# Measured on qml6 6.12 with QtQuick3D: the `time` of a system in a hidden
# node goes on. What a hidden system shows is nothing, so how often its
# particles are worked out while hidden is not to be seen; scheduling is
# Ljós's own (0 % CPU when idle).

@ljos-only
Feature: A hidden particle system
  A system that is hidden, itself or by a node it is in, shows nothing, so it
  asks for no frame and works nothing out. Its time still goes on: shown
  again, it is where it would have been. Something that reads its time
  keeps it going while hidden, and a script that reads it sees it go on.

  # An orthographic camera, so one unit is one pixel, and y goes up. One
  # particle leaves the middle upwards at 100 pixels a second.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        id: root; objectName: "root"; width: 200; height: 200; color: "black"
        property bool reads: false
        property int seen: reads ? system.time : -1
        property int looked: -1
        function look() { looked = system.time }
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          Node {
            objectName: "holder"
            ParticleSystem3D {
              id: system; objectName: "system"; useRandomSeed: false
              SpriteParticle3D { id: grey; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone }
              ParticleEmitter3D {
                particle: grey; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) }
                EmitBurst3D { amount: 1 } }
            }
          }
        }
      }
      """

  Scenario: A hidden system schedules nothing
    When 100 milliseconds pass
    And the property "visible" of "system" is set to false
    And 100 milliseconds pass
    Then the application has nothing scheduled

  Scenario: A system in a hidden node schedules nothing
    When 100 milliseconds pass
    And the property "visible" of "holder" is set to false
    And 100 milliseconds pass
    Then the application has nothing scheduled

  Scenario: Shown again, a system is where its time would have taken it
    When 100 milliseconds pass
    And the property "visible" of "holder" is set to false
    And 400 milliseconds pass
    And the property "visible" of "holder" is set to true
    And 10 milliseconds pass
    Then the property "time" of "system" is at least 500
    And the pixel at 100, 50 of the window is within 4 of "#808080"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: What reads the time of a hidden system keeps it going
    When 100 milliseconds pass
    And the property "visible" of "system" is set to false
    And the property "reads" of "root" is set to true
    And 300 milliseconds pass
    Then the property "seen" of "root" is at least 350

  Scenario: What starts reading the time of a hidden system that has gone quiet keeps it going
    When 100 milliseconds pass
    And the property "visible" of "system" is set to false
    And 200 milliseconds pass
    Then the application has nothing scheduled
    When the property "reads" of "root" is set to true
    And 300 milliseconds pass
    Then the property "seen" of "root" is at least 550

  Scenario: A stopped system keeps the time it was given when it is hidden
    When 100 milliseconds pass
    And the property "running" of "system" is set to false
    And the property "time" of "system" is set to 1000
    And the property "visible" of "system" is set to false
    And 100 milliseconds pass
    Then the property "time" of "system" is 1000

  Scenario: A system paused, hidden and let go on sees its time go on
    When 100 milliseconds pass
    And the property "paused" of "system" is set to true
    And the property "visible" of "system" is set to false
    And 100 milliseconds pass
    And the property "paused" of "system" is set to false
    And 300 milliseconds pass
    And the method "look" of "root" is called
    Then the property "looked" of "root" is at least 350
    And the application has nothing scheduled

  Scenario: A hidden system paused keeps the time it ran while hidden
    When 100 milliseconds pass
    And the property "visible" of "system" is set to false
    And 200 milliseconds pass
    And the property "paused" of "system" is set to true
    And 100 milliseconds pass
    And the method "look" of "root" is called
    Then the property "looked" of "root" is at least 280
    And the property "looked" of "root" is at most 320

  Scenario: A script that reads the time of a hidden system sees it go on
    When 100 milliseconds pass
    And the property "visible" of "system" is set to false
    And 400 milliseconds pass
    And the method "look" of "root" is called
    Then the property "looked" of "root" is at least 500
    And the application has nothing scheduled
