# Sources:
#   https://doc.qt.io/qt-6/qtquick-particles-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-particlesystem.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-emitter.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-imageparticle.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-pointdirection.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-angledirection.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-targetdirection.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-ellipseshape.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-maskshape.html
#   https://doc.qt.io/qt-6/qml-qtquick-particles-wander.html
# Verified on Qt 6.11.2 (qml6, offscreen).
#
# Where a particle is born is random on Qt, so a scenario states only what no
# draw of the dice can change: defaults, whether the system has particles at
# all, and a pixel that a particle must cover (the picture is far larger than
# the emitter). Ljós seeds its dice from a fixed number, so its runs repeat;
# Qt's randomness is not a spec.

Feature: Particles
  A ParticleSystem keeps the particles its Emitters make and the
  ImageParticles that draw them.

  Background:
    Given a 8 by 8 image file "dot.png" filled with "#ff0000"

  @qt-parity
  Scenario Outline: Defaults
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      Item {
        ParticleSystem { objectName: "bare" }
        Emitter { objectName: "emitter" }
        ImageParticle { objectName: "painter" }
        PointDirection { objectName: "point" }
        AngleDirection { objectName: "angle" }
        TargetDirection { objectName: "target" }
        EllipseShape { objectName: "ellipse" }
        Wander { objectName: "wander" }
        Age { objectName: "age" }
      }
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object  | property              | value |
      | bare    | running               | true  |
      | bare    | paused                | false |
      | bare    | empty                 | true  |
      | emitter | enabled               | true  |
      | emitter | emitRate              | 10    |
      | emitter | lifeSpan              | 1000  |
      | emitter | lifeSpanVariation     | 0     |
      | emitter | maximumEmitted        | -1    |
      | emitter | startTime             | 0     |
      | emitter | size                  | 16    |
      | emitter | endSize               | -1    |
      | emitter | sizeVariation         | 0     |
      | emitter | group                 | ""    |
      | painter | alpha                 | 1     |
      | painter | alphaVariation        | 0     |
      | painter | colorVariation        | 0     |
      | painter | entryEffect           | 1     |
      | point   | x                     | 0     |
      | point   | y                     | 0     |
      | point   | xVariation            | 0     |
      | point   | yVariation            | 0     |
      | angle   | angle                 | 0     |
      | angle   | magnitude             | 0     |
      | angle   | angleVariation        | 0     |
      | angle   | magnitudeVariation    | 0     |
      | target  | targetX               | 0     |
      | target  | targetY               | 0     |
      | target  | targetVariation       | 0     |
      | target  | magnitude             | 0     |
      | target  | magnitudeVariation    | 0     |
      | target  | proportionalMagnitude | false |
      | ellipse | fill                  | true  |
      | wander  | pace                  | 0     |
      | wander  | xVariance             | 0     |
      | wander  | yVariance             | 0     |
      | age     | lifeLeft              | 0     |
      | age     | advancePosition       | true  |

  @qt-parity
  Scenario: An emitter inside a system makes particles
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        objectName: "system"
        width: 300; height: 300
        Emitter { emitRate: 100; lifeSpan: 5000; width: 10; height: 10 }
        ImageParticle { source: "dot.png" }
      }
      """
    When 200 milliseconds pass
    Then the property "empty" of "system" is false

  @qt-parity
  Scenario: The painter draws its particles without items of its own
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        objectName: "system"
        width: 300; height: 300
        Emitter { emitRate: 100; lifeSpan: 5000; width: 10; height: 10 }
        ImageParticle {
          objectName: "painter"
          source: "dot.png"
          property int items: children.length
        }
      }
      """
    When 200 milliseconds pass
    Then the property "empty" of "system" is false
    And the property "items" of "painter" is 0

  @qt-parity
  Scenario: A disabled emitter makes none
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        objectName: "system"
        width: 300; height: 300
        Emitter { enabled: false; emitRate: 100; width: 10; height: 10 }
        ImageParticle { source: "dot.png" }
      }
      """
    When 200 milliseconds pass
    Then the property "empty" of "system" is true

  @qt-parity
  Scenario: A paused system makes none
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        objectName: "system"
        paused: true
        width: 300; height: 300
        Emitter { emitRate: 100; width: 10; height: 10 }
        ImageParticle { source: "dot.png" }
      }
      """
    When 200 milliseconds pass
    Then the property "empty" of "system" is true

  @qt-parity
  Scenario: A burst emits even from a disabled emitter, and the particles die
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        objectName: "system"
        width: 300; height: 300
        Emitter {
          objectName: "emitter"
          enabled: false; lifeSpan: 200; width: 10; height: 10
        }
        ImageParticle { source: "dot.png" }
      }
      """
    When the method "burst" of "emitter" is called with 5
    And 50 milliseconds pass
    Then the property "empty" of "system" is false
    When 1 second passes
    Then the property "empty" of "system" is true

  # Qt's software renderer, which qml6 uses offscreen, draws no particles at
  # all, so what is painted is Ljós's own behaviour here.
  @ljos-only
  Scenario: A particle is drawn over the emitter, centred
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        width: 300; height: 300
        Emitter {
          x: 140; y: 140; width: 20; height: 20
          emitRate: 200; lifeSpan: 5000; size: 100
        }
        ImageParticle { source: "dot.png"; entryEffect: ImageParticle.None }
      }
      """
    When 200 milliseconds pass
    Then the pixel at 150, 150 of the window is "#ff0000"
    And the pixel at 10, 10 of the window is "#000000"

  # Qt's software renderer, which qml6 uses offscreen, draws no particles at
  # all, so what is painted is Ljós's own behaviour here.
  @ljos-only
  Scenario: Particles of another group are not drawn
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        width: 300; height: 300
        Emitter {
          x: 140; y: 140; width: 20; height: 20
          group: "sparks"; emitRate: 200; lifeSpan: 5000; size: 100
        }
        ImageParticle { source: "dot.png"; entryEffect: ImageParticle.None }
      }
      """
    When 200 milliseconds pass
    Then the pixel at 150, 150 of the window is "#000000"

  # Qt's software renderer, which qml6 uses offscreen, draws no particles at
  # all, so what is painted is Ljós's own behaviour here.
  @ljos-only
  Scenario: A painter draws the groups it names
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        width: 300; height: 300
        Emitter {
          x: 140; y: 140; width: 20; height: 20
          group: "sparks"; emitRate: 200; lifeSpan: 5000; size: 100
        }
        ImageParticle {
          source: "dot.png"; groups: ["sparks"]
          entryEffect: ImageParticle.None
        }
      }
      """
    When 1 second passes
    Then the pixel at 150, 150 of the window is "#ff0000"

  # Qt's software renderer, which qml6 uses offscreen, draws no particles at
  # all, so what is painted is Ljós's own behaviour here.
  @ljos-only
  Scenario Outline: A velocity carries particles away from the emitter
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        width: 300; height: 300
        Emitter {
          x: 0; y: 140; width: 10; height: 20
          emitRate: 100; lifeSpan: 5000; size: 100
          velocity: <direction>
        }
        ImageParticle { source: "dot.png"; entryEffect: ImageParticle.None }
      }
      """
    When 1 second passes
    Then the pixel at 150, 150 of the window is "<pixel>"

    Examples:
      | direction                                         | pixel   |
      | PointDirection { x: 2000 }                        | #ff0000 |
      | AngleDirection { angle: 0; magnitude: 2000 }      | #ff0000 |
      | TargetDirection { targetX: 300; magnitude: 2000 } | #ff0000 |

  # Qt's software renderer, which qml6 uses offscreen, draws no particles at
  # all, so what is painted is Ljós's own behaviour here.
  @ljos-only
  Scenario: An ellipse shape leaves the corners of the emitter empty
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        width: 300; height: 300
        Emitter {
          x: 100; y: 100; width: 100; height: 100
          emitRate: 400; lifeSpan: 5000; size: 4
          shape: EllipseShape { fill: true }
        }
        ImageParticle { source: "dot.png"; entryEffect: ImageParticle.None }
      }
      """
    When 1 second passes
    Then the pixel at 100, 100 of the window is "#000000"
    And the pixel at 199, 199 of the window is "#000000"

  # Qt's `empty` did not turn true in a qml6 probe of this, whatever the Age's
  # size; the expectation is the documentation's ("lifeLeft"), not an oracle's.
  Scenario: An Age makes the particles it reaches die sooner
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Particles
      ParticleSystem {
        id: system
        objectName: "system"
        width: 300; height: 300
        Emitter {
          objectName: "emitter"
          enabled: false; lifeSpan: 3000; width: 10; height: 10
        }
        Age { lifeLeft: 100; system: system; width: 300; height: 300 }
        ImageParticle { source: "dot.png" }
      }
      """
    When the method "burst" of "emitter" is called with 5
    And 1 second passes
    Then the property "empty" of "system" is true
