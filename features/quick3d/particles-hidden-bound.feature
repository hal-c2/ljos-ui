# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particlesystem3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-attractor3d.html
# Qt Design Studio writes an affector's duration as a binding on its
# system's time (FX Material Showroom's shockwave). Measured on qml6 6.12:
# the time of a hidden system goes on, and so does what is bound to it.
# How often a binding nobody reads is worked out is not to be seen;
# scheduling is Ljós's own (0 % CPU when idle). The scenarios run bindings
# as `ljos run` does, worked out when read: the test backend otherwise works
# each one out after every change, to report binding loops when Qt does.

@ljos-only
Feature: What is bound to the time of a hidden particle system
  A binding on the time of a hidden system, read by nothing that shows or
  does anything, keeps no clock going: the system asks for no frame. Read,
  it gives what the time has become; shown again, the system is where its
  time would have taken it; and something that comes to watch it keeps the
  time going.

  Background:
    Given the application works out a binding only when it is read
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        id: root; objectName: "root"; width: 200; height: 200; color: "black"
        property bool watches: false
        property int looked: -1
        function look() { looked = pull.duration }
        // Drawn: what its width is bound to is seen.
        Rectangle { objectName: "bar"; height: 1; width: root.watches ? pull.duration / 1000 : 0 }
        View3D {
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          Node { objectName: "holder"
            ParticleSystem3D { id: system; objectName: "system"; useRandomSeed: false
              SpriteParticle3D { id: grey; color: "#808080"; particleScale: 20 }
              ParticleEmitter3D { particle: grey; lifeSpan: 100000; EmitBurst3D { amount: 1 } }
              Attractor3D { id: pull; objectName: "pull"; particles: [grey]; system: system
                position: Qt.vector3d(0, 1000, 0); duration: system.time * 2 + 100000 }
            }
          }
        }
      }
      """

  Scenario: A hidden system whose time only a binding reads schedules nothing
    When 100 milliseconds pass
    And the property "visible" of "holder" is set to false
    And 100 milliseconds pass
    Then the application has nothing scheduled

  Scenario: A script reads what is bound to the time of a hidden system as the time has gone on
    When 100 milliseconds pass
    And the property "visible" of "holder" is set to false
    And 400 milliseconds pass
    And the method "look" of "root" is called
    Then the property "looked" of "root" is at least 101000
    When 300 milliseconds pass
    And the method "look" of "root" is called
    Then the property "looked" of "root" is at least 101600
    And the application has nothing scheduled

  Scenario: What comes to watch a binding on the time of a hidden system keeps the time going
    When 100 milliseconds pass
    And the property "visible" of "holder" is set to false
    And 200 milliseconds pass
    Then the application has nothing scheduled
    When the property "watches" of "root" is set to true
    And 300 milliseconds pass
    Then the property "width" of "bar" is at least 101.1

  Scenario: Shown again, a system whose time a binding reads is where its time would have taken it
    When 100 milliseconds pass
    And the property "visible" of "holder" is set to false
    And 400 milliseconds pass
    And the property "visible" of "holder" is set to true
    And 10 milliseconds pass
    Then the property "time" of "system" is at least 500
    And the property "duration" of "pull" is at least 101000

  Scenario: The inspector shows the clock of a hidden system riding along
    Given the inspector is connected
    When 100 milliseconds pass
    And the property "visible" of "holder" is set to false
    And 100 milliseconds pass
    And the agent calls the tool "app_processes" with:
      """json
      {"kind": "waker"}
      """
    Then the answer's "/scheduled" is 0
    And the answer's "/processes" has 1 entries
    And the answer's "/processes/0/source" is "animation"
    And the answer's "/processes/0/pace" is "riding_along"
    And the answer's text contains "rides along with the frames of others"
