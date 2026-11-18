// The car in 3D on a plinth, from a camera that orbits it. The car itself
// is CarModel.qml.
//
// This file stays in the QtQuick3D vocabulary: Ljós runs it as it is.
import QtQuick
import QtQuick3D
import QtQuick3D.Helpers

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

    CarModel {
        doorLeft: scene.doorLeft
        doorRight: scene.doorRight
        frunk: scene.frunk
        trunk: scene.trunk
        chargePort: scene.chargePort
        roll: scene.roll
        steer: scene.steer
        stripTint: scene.stripTint
        tailTint: scene.tailTint
        strips: scene.strips
    }
}
