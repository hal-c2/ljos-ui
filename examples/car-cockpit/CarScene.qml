// The car in 3D. The parts are separate files from assets/car (made by
// build_car.py); each sits under a Node at the pivot rig.json gives and
// opens by turning that node about rig.json's axis. The light strips are
// emissive boxes, so the scene's glow blooms them and nothing else.
//
// This file stays in the QtQuick3D vocabulary: Ljós runs it as it is.
import QtQuick
import QtQuick3D
import QtQuick3D.Helpers
import QtQuick3D.AssetUtils

View3D {
    id: scene

    // How far open each part is, 0 to 1. The panel animates these.
    property real doorLeft: 0
    property real doorRight: 0
    property real frunk: 0
    property real trunk: 0
    property real chargePort: 0
    // Wheel turn in degrees, and steering in degrees.
    property real roll: 0
    property real steer: 0
    // Where the camera looks from, in degrees round the car.
    property real yaw: -32
    property real pitch: -16
    property real distance: 7.8
    property bool bloom: true
    property bool dark: true
    // The plinth's top.
    property color plinth: "#151b28"
    property color stripTint: "#00e5ff"
    property color tailTint: "#ff0d59"
    // How bright the strips are: they swell while the car is busy.
    property real strips: 1

    // A tap on the car names the file node under it: "door_left", "body"…
    signal picked(string node)
    // The keyboard's way round the car: arrows ask to aim at the next
    // part, Space or Enter picks the one aimed at.
    signal aimed(string node)
    readonly property var openable: ["door_left", "door_right", "frunk", "trunk", "charge_port"]
    // The part the car is aimed at, set by whoever owns the focus. What
    // the model is called and what pressing it does both read this, so
    // they cannot disagree.
    property string aim: "door_left"
    property string aimName: ""
    // What it is and how to use it, in the app's language.
    property string help: ""

    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: scene.aimName
    Accessible.description: scene.help
    Accessible.onPressAction: scene.picked(scene.aim)
    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Right || event.key === Qt.Key_Left) {
            const step = event.key === Qt.Key_Right ? 1 : scene.openable.length - 1
            const at = Math.max(0, scene.openable.indexOf(scene.aim))
            scene.aimed(scene.openable[(at + step) % scene.openable.length])
            event.accepted = true
        } else if (event.key === Qt.Key_Space || event.key === Qt.Key_Return) {
            scene.picked(scene.aim)
            event.accepted = true
        }
    }

    TapHandler {
        onTapped: (point) => {
            const hit = scene.pick(point.position.x, point.position.y)
            if (hit.objectHit)
                scene.picked(hit.objectHit.objectName)
        }
    }

    // rig.json's scale is metres; the scene's unit is the centimetre.
    readonly property real m: 100

    environment: ExtendedSceneEnvironment {
        // The panel shows through around the plinth.
        backgroundMode: SceneEnvironment.Transparent
        antialiasingMode: SceneEnvironment.MSAA
        antialiasingQuality: SceneEnvironment.High
        tonemapMode: SceneEnvironment.TonemapModeFilmic
        glowEnabled: scene.bloom
        glowStrength: 1.1
        glowIntensity: scene.dark ? 0.9 : 0.5
        glowBloom: 0.25
        glowHDRMinimumValue: 1.2
        glowQualityHigh: true
        glowUseBicubicUpscale: true
        glowLevel: ExtendedSceneEnvironment.GlowLevel.Two
            | ExtendedSceneEnvironment.GlowLevel.Three
            | ExtendedSceneEnvironment.GlowLevel.Four
    }

    Node {
        id: orbit
        y: 0.3 * scene.m
        eulerRotation.y: scene.yaw
        Node {
            eulerRotation.x: scene.pitch
            PerspectiveCamera {
                id: camera
                z: scene.distance * scene.m
                fieldOfView: 30
                clipNear: 10
                clipFar: 10000
            }
        }
    }

    DirectionalLight {
        eulerRotation.x: -48
        eulerRotation.y: -30
        brightness: scene.dark ? 2.4 : 1.5
        castsShadow: true
        shadowFactor: 40
    }
    DirectionalLight {
        eulerRotation.x: -40
        eulerRotation.y: 160
        color: scene.dark ? "#c04ab8" : "#ffffff"
        brightness: scene.dark ? 0.7 : 0.5
    }
    // Fill from the camera's side, so the flanks are not black.
    DirectionalLight {
        eulerRotation.x: -10
        eulerRotation.y: scene.yaw - 20
        color: scene.dark ? "#6fa8c8" : "#ffffff"
        brightness: scene.dark ? 1.5 : 0.8
    }

    // The plinth the car stands on, to catch its shadow: a low disc, so the
    // car reads as on show rather than lost on a dark floor.
    Model {
        source: "#Cylinder"
        y: -1
        scale: Qt.vector3d(6.4, 0.02, 6.4)
        receivesShadows: true
        pickable: false
        materials: PrincipledMaterial {
            baseColor: scene.plinth
            roughness: 0.9
        }
    }
    // A lit rim just below the plinth's edge.
    Model {
        source: "#Cylinder"
        y: -1.6
        scale: Qt.vector3d(6.48, 0.004, 6.48)
        pickable: false
        castsShadows: false
        materials: PrincipledMaterial {
            baseColor: "black"
            emissiveFactor: Qt.vector3d(scene.stripTint.r * 1.3 * scene.strips,
                                        scene.stripTint.g * 1.3 * scene.strips,
                                        scene.stripTint.b * 1.3 * scene.strips)
        }
    }

    Node {
        id: car
        scale: Qt.vector3d(scene.m, scene.m, scene.m)

        RuntimeLoader {
            source: "assets/car/body.glb"
        }

        Node {
            position: Qt.vector3d(0.34, 1.245, -0.135)
            eulerRotation.z: 70 * scene.doorLeft
            RuntimeLoader {
                source: "assets/car/door_left.glb"
            }
            Strip {
                position: Qt.vector3d(0.6281, -0.465, 0)
                size: Qt.vector3d(0.012, 0.014, 0.97)
                tint: scene.stripTint
                level: scene.strips
            }
        }
        Node {
            position: Qt.vector3d(-0.34, 1.245, -0.135)
            eulerRotation.z: -70 * scene.doorRight
            RuntimeLoader {
                source: "assets/car/door_right.glb"
            }
            Strip {
                position: Qt.vector3d(-0.6281, -0.465, 0)
                size: Qt.vector3d(0.012, 0.014, 0.97)
                tint: scene.stripTint
                level: scene.strips
            }
        }
        Node {
            position: Qt.vector3d(0, 0.9101, 0.92)
            eulerRotation.x: -55 * scene.frunk
            RuntimeLoader {
                source: "assets/car/frunk.glb"
            }
        }
        Node {
            position: Qt.vector3d(0, 1.2088, -0.8)
            eulerRotation.x: 60 * scene.trunk
            RuntimeLoader {
                source: "assets/car/trunk.glb"
            }
        }
        Node {
            position: Qt.vector3d(0.9388, 0.7852, -1.95)
            eulerRotation.y: -95 * scene.chargePort
            RuntimeLoader {
                source: "assets/car/charge_port.glb"
            }
        }

        // The wheels: the left ones as the file has them, the right ones
        // turned round, so they roll the other way about their own axis.
        Node {
            position: Qt.vector3d(0.83, 0.37, 1.4)
            eulerRotation.y: scene.steer
            Node {
                eulerRotation.x: scene.roll
                RuntimeLoader {
                    source: "assets/car/wheel.glb"
                }
            }
        }
        Node {
            position: Qt.vector3d(-0.83, 0.37, 1.4)
            eulerRotation.y: 180 + scene.steer
            Node {
                eulerRotation.x: -scene.roll
                RuntimeLoader {
                    source: "assets/car/wheel.glb"
                }
            }
        }
        Node {
            position: Qt.vector3d(0.83, 0.37, -1.4)
            eulerRotation.x: scene.roll
            RuntimeLoader {
                source: "assets/car/wheel.glb"
            }
        }
        Node {
            position: Qt.vector3d(-0.83, 0.37, -1.4)
            eulerRotation.y: 180
            Node {
                eulerRotation.x: -scene.roll
                RuntimeLoader {
                    source: "assets/car/wheel.glb"
                }
            }
        }

        Strip {
            position: Qt.vector3d(0, 0.43, 2.304)
            size: Qt.vector3d(1.56, 0.03, 0.012)
            tint: scene.stripTint
            level: scene.strips
        }
        Strip {
            position: Qt.vector3d(0, 0.8, -2.304)
            size: Qt.vector3d(1.64, 0.04, 0.012)
            tint: scene.tailTint
            level: scene.strips
        }
        Strip {
            position: Qt.vector3d(0.932, 0.3, 0)
            size: Qt.vector3d(0.012, 0.022, 1.7)
            tint: scene.stripTint
            level: scene.strips
        }
        Strip {
            position: Qt.vector3d(-0.932, 0.3, 0)
            size: Qt.vector3d(0.012, 0.022, 1.7)
            tint: scene.stripTint
            level: scene.strips
        }
    }
}
