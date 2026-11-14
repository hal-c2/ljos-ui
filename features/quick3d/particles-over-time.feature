# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-particles3d-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particlesystem3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particleemitter3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-lineparticle3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-pointrotator3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-repeller3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-attractor3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-targetdirection3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-trailemitter3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-dynamicburst3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-spriteparticle3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-modelparticle3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-gravity3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-emitburst3d.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: What happens to particles over time
  A system that is not running is where its `time` says, so what a particle
  has done by then can be measured. One unit is one pixel, and y goes up.

  Scenario: A particle goes at its velocity
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 50 of the window is within 4 of "#808080"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: Gravity pulls a particle down
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }  EmitBurst3D { amount: 1 } }
            Gravity3D { magnitude: 400 }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 150 of the window is within 4 of "#808080"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A particle is gone at the end of its life
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 300; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) } EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A particle fades in
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeOpacity; fadeInDuration: 1000; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 100 of the window is within 4 of "#404040"

  Scenario: A particle fades to nothing by scale
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeScale; fadeInDuration: 1000; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 100 of the window is within 4 of "#808080"
    And the pixel at 93, 100 of the window is within 4 of "#000000"

  Scenario: An emitter makes its rate
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 4; maxAmount: 100; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; emitRate: 10; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 1000
    Then the pixel at 100, 60 of the window is within 4 of "#808080"
    And the pixel at 100, 90 of the window is within 4 of "#808080"

  Scenario: An affector for another kind leaves this one alone
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            SpriteParticle3D { id: other }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }  EmitBurst3D { amount: 1 } }
            Gravity3D { magnitude: 400; particles: [other] }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 100 of the window is within 4 of "#808080"

  Scenario: A disabled affector does nothing
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }  EmitBurst3D { amount: 1 } }
            Gravity3D { magnitude: 400; enabled: false }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 100 of the window is within 4 of "#808080"

  Scenario: A rotator turns a particle around its axis
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; x: 50; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) } EmitBurst3D { amount: 1 } }
            PointRotator3D { magnitude: 180; direction: Qt.vector3d(0, 0, 1) }
          }
        }
      }
      """
    When the property "time" of "system" is set to 1000
    Then the pixel at 50, 100 of the window is within 4 of "#808080"
    And the pixel at 150, 100 of the window is within 4 of "#000000"

  Scenario: A repeller pushes a particle out to its radius
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; x: 10; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) } EmitBurst3D { amount: 1 } }
            Repeller3D { radius: 50; outerRadius: 50; strength: 100 }
          }
        }
      }
      """
    When the property "time" of "system" is set to 2000
    Then the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: An attractor brings a particle to its place by the end of its life
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone; maxAmount: 10 }
            ParticleEmitter3D { particle: kind; lifeSpan: 1000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) } EmitBurst3D { amount: 1 } }
            Attractor3D { x: 60; duration: 1000 }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 130, 100 of the window is within 4 of "#808080"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A target direction sends a particle to a place
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone; maxAmount: 10 }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: TargetDirection3D { position: Qt.vector3d(0, 50, 0); normalized: false } EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 1000
    Then the pixel at 100, 50 of the window is within 4 of "#808080"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A dynamic burst has not come before its time
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone; maxAmount: 10 }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; emitRate: 0; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) } DynamicBurst3D { amount: 1; time: 200 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 100
    Then the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A model particle shows its delegate with the colour of the kind
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            ModelParticle3D { id: kind; color: "#ff0000"; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone
              delegate: Component { Model { source: "#Rectangle"; scale: Qt.vector3d(0.4, 0.4, 1); materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" } } } }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 50 of the window is within 4 of "#ff0000"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A model written in a model particle is its delegate
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            ModelParticle3D { id: kind; color: "#ff0000"; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone
              Model { source: "#Rectangle"; scale: Qt.vector3d(0.4, 0.4, 1); materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" } } }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 50 of the window is within 4 of "#ff0000"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A model anywhere shows the table of the kind
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            ModelParticle3D { id: kind; color: "#ff0000"; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) }  EmitBurst3D { amount: 1 } }
            Model { source: "#Rectangle"; scale: Qt.vector3d(0.4, 0.4, 1); instancing: kind.instanceTable; materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 500
    Then the pixel at 100, 50 of the window is within 4 of "#ff0000"
    And the pixel at 100, 100 of the window is within 4 of "#000000"

  Scenario: A line goes from a particle back to where it was
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            useRandomSeed: false
            LineParticle3D { id: kind; color: "#808080"; particleScale: 10; segmentCount: 5; length: 40; lengthDeltaMin: 1; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When 700 milliseconds pass
    And the property "paused" of "system" is set to true
    And 50 milliseconds pass
    Then the pixel at 100, 50 of the window is within 4 of "#808080"
    And the pixel at 115, 50 of the window is within 4 of "#000000"
    And the pixel at 100, 90 of the window is within 4 of "#000000"

  Scenario: Screen adds to what is behind
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#404040"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#404040"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone; blendMode: SpriteParticle3D.Screen }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 100
    Then the pixel at 100, 100 of the window is within 4 of "#808080"

  Scenario: Multiply darkens what is behind
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#808080"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            running: false
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone; blendMode: SpriteParticle3D.Multiply }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) }  EmitBurst3D { amount: 1 } }
          }
        }
      }
      """
    When the property "time" of "system" is set to 100
    Then the pixel at 100, 100 of the window is within 4 of "#404040"
    And the pixel at 60, 100 of the window is within 4 of "#808080"

  Scenario: A trail emitter makes particles where another kind is
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      import QtQuick3D.Particles3D
      Rectangle {
        width: 200; height: 200
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          DirectionalLight { brightness: 0 }
          ParticleSystem3D {
            objectName: "system"
            useRandomSeed: false
            SpriteParticle3D { id: kind; color: "#808080"; particleScale: 20; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone;  }
            SpriteParticle3D { id: smoke; color: "#ff0000"; particleScale: 10; fadeInEffect: Particle3D.FadeNone; fadeOutEffect: Particle3D.FadeNone }
            ParticleEmitter3D { particle: kind; lifeSpan: 100000; velocity: VectorDirection3D { direction: Qt.vector3d(0, 100, 0) }  EmitBurst3D { amount: 1 } }
            TrailEmitter3D { particle: smoke; follow: kind; lifeSpan: 100000; emitRate: 40; velocity: VectorDirection3D { direction: Qt.vector3d(0, 0, 0) } }
          }
        }
      }
      """
    When 500 milliseconds pass
    Then the pixel at 100, 80 of the window is within 40 of "#ff0000"
