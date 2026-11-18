// The car, as build_car.py rigs it: each part is a file from assets/car
// under a Node at the pivot rig.json gives, and opens by turning that node
// about rig.json's axis. The light strips are emissive boxes, so a scene's
// glow blooms them and nothing else. The car panel shows it on a plinth;
// the cluster shows it from behind on the road.
//
// This file stays in the QtQuick3D vocabulary: Ljós runs it as it is.
import QtQuick
import QtQuick3D
import QtQuick3D.AssetUtils

Node {
    id: model

    // How far open each part is, 0 to 1.
    property real doorLeft: 0
    property real doorRight: 0
    property real frunk: 0
    property real trunk: 0
    property real chargePort: 0
    // Wheel turn in degrees, and steering in degrees.
    property real roll: 0
    property real steer: 0
    property color stripTint: "#00e5ff"
    property color tailTint: "#ff0d59"
    // How bright the strips are.
    property real strips: 1
    // rig.json's scale is metres; the scene's unit is the centimetre.
    readonly property real m: 100

    scale: Qt.vector3d(model.m, model.m, model.m)

    RuntimeLoader {
        source: "assets/car/body.glb"
    }

    Node {
        position: Qt.vector3d(0.355, 1.205, -0.135)
        eulerRotation.z: 70 * model.doorLeft
        RuntimeLoader {
            source: "assets/car/door_left.glb"
        }
        Strip {
            position: Qt.vector3d(0.6415, -0.645, 0)
            size: Qt.vector3d(0.012, 0.014, 0.97)
            tint: model.stripTint
            level: model.strips
        }
    }
    Node {
        position: Qt.vector3d(-0.355, 1.205, -0.135)
        eulerRotation.z: -70 * model.doorRight
        RuntimeLoader {
            source: "assets/car/door_right.glb"
        }
        Strip {
            position: Qt.vector3d(-0.6415, -0.645, 0)
            size: Qt.vector3d(0.012, 0.014, 0.97)
            tint: model.stripTint
            level: model.strips
        }
    }
    Node {
        position: Qt.vector3d(0, 0.8583, 0.92)
        eulerRotation.x: -55 * model.frunk
        RuntimeLoader {
            source: "assets/car/frunk.glb"
        }
    }
    Node {
        position: Qt.vector3d(0, 1.1465, -0.8)
        eulerRotation.x: 60 * model.trunk
        RuntimeLoader {
            source: "assets/car/trunk.glb"
        }
    }
    Node {
        position: Qt.vector3d(1.005, 0.7734, -1.95)
        eulerRotation.y: -95 * model.chargePort
        RuntimeLoader {
            source: "assets/car/charge_port.glb"
        }
    }

    // The wheels: the left ones as the file has them, the right ones
    // turned round, so they roll the other way about their own axis.
    Node {
        position: Qt.vector3d(0.83, 0.37, 1.4)
        eulerRotation.y: model.steer
        Node {
            eulerRotation.x: model.roll
            RuntimeLoader {
                source: "assets/car/wheel.glb"
            }
        }
    }
    Node {
        position: Qt.vector3d(-0.83, 0.37, 1.4)
        eulerRotation.y: 180 + model.steer
        Node {
            eulerRotation.x: -model.roll
            RuntimeLoader {
                source: "assets/car/wheel.glb"
            }
        }
    }
    Node {
        position: Qt.vector3d(0.83, 0.37, -1.4)
        eulerRotation.x: model.roll
        RuntimeLoader {
            source: "assets/car/wheel.glb"
        }
    }
    Node {
        position: Qt.vector3d(-0.83, 0.37, -1.4)
        eulerRotation.y: 180
        Node {
            eulerRotation.x: -model.roll
            RuntimeLoader {
                source: "assets/car/wheel.glb"
            }
        }
    }

    Strip {
        position: Qt.vector3d(0, 0.37, 2.304)
        size: Qt.vector3d(1.6, 0.035, 0.012)
        tint: model.stripTint
        level: model.strips
    }
    Strip {
        position: Qt.vector3d(0, 0.79, -2.304)
        size: Qt.vector3d(1.74, 0.045, 0.012)
        tint: model.tailTint
        level: model.strips
    }
    Strip {
        position: Qt.vector3d(0.9552, 0.3, 0)
        size: Qt.vector3d(0.012, 0.022, 1.7)
        tint: model.stripTint
        level: model.strips
    }
    Strip {
        position: Qt.vector3d(-0.9552, 0.3, 0)
        size: Qt.vector3d(0.012, 0.022, 1.7)
        tint: model.stripTint
        level: model.strips
    }
}
