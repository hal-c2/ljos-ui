// Ten thousand cubes, one instanced model whose table a RandomInstancing
// made, turned as a whole: what an instanced draw costs when every copy
// moves on screen while its table does not change.
import QtQuick
import QtQuick3D
import QtQuick3D.Helpers

Window {
    id: root
    width: 640
    height: 480
    visible: true
    color: "#202030"

    property real angle: 0
    NumberAnimation on angle {
        from: 0
        to: 360
        duration: 8000
        loops: Animation.Infinite
    }

    View3D {
        anchors.fill: parent
        environment: SceneEnvironment {
            clearColor: "#202030"
            backgroundMode: SceneEnvironment.Color
        }
        PerspectiveCamera { z: 1600 }
        DirectionalLight { eulerRotation.x: -45 }
        Node {
            eulerRotation.y: root.angle
            Model {
                source: "#Cube"
                scale: Qt.vector3d(0.1, 0.1, 0.1)
                materials: DefaultMaterial { diffuseColor: "white" }
                instancing: RandomInstancing {
                    instanceCount: 10000
                    randomSeed: 2024
                    position: InstanceRange {
                        from: Qt.vector3d(-600, -400, -600)
                        to: Qt.vector3d(600, 400, 600)
                    }
                    rotation: InstanceRange {
                        from: Qt.vector3d(0, 0, 0)
                        to: Qt.vector3d(360, 360, 360)
                    }
                    color: InstanceRange { from: "#4060ff"; to: "#ffa040" }
                }
            }
        }
    }
}
