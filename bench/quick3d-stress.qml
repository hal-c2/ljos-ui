// A scene that is many things at once, for what a frame costs when none of
// them is the only cost: 480 models of five meshes, glTF models read while
// the window opens, twelve lights of which four cast shadows, three
// textures of which one is 2048 by 2048, mixed materials with blended ones
// among them, and a camera orbiting under a NumberAnimation, so every frame
// draws everything again from a new place. The textures are written by
// bench/quick3d/gen-stress.py. Plain QtQuick3D, so qml6 runs it unchanged.
import QtQuick
import QtQuick3D
import QtQuick3D.AssetUtils

Window {
    id: root
    width: 800
    height: 600
    visible: true
    color: "#101018"

    property real orbit: 0
    NumberAnimation on orbit {
        from: 0
        to: 360
        duration: 12000
        loops: Animation.Infinite
    }

    readonly property var meshes: ["#Cube", "#Sphere", "#Cylinder", "#Cone", "#Rectangle"]

    View3D {
        id: view
        anchors.fill: parent
        environment: SceneEnvironment {
            clearColor: "#101018"
            backgroundMode: SceneEnvironment.Color
            antialiasingMode: SceneEnvironment.NoAA
        }

        Node {
            eulerRotation.y: root.orbit
            PerspectiveCamera {
                y: 450
                z: 1300
                eulerRotation.x: -20
                clipFar: 5000
            }
        }

        // Four lights that cast shadows.
        DirectionalLight {
            eulerRotation: Qt.vector3d(-55, 35, 0)
            brightness: 0.5
            castsShadow: true
            shadowMapQuality: Light.ShadowMapQualityHigh
        }
        SpotLight {
            position: Qt.vector3d(-500, 600, 300)
            eulerRotation: Qt.vector3d(-60, -40, 0)
            coneAngle: 70
            quadraticFade: 0
            brightness: 0.6
            color: "#ffd0a0"
            castsShadow: true
        }
        SpotLight {
            position: Qt.vector3d(500, 600, -300)
            eulerRotation: Qt.vector3d(-60, 140, 0)
            coneAngle: 70
            quadraticFade: 0
            brightness: 0.6
            color: "#a0d0ff"
            castsShadow: true
        }
        PointLight {
            position: Qt.vector3d(0, 400, 0)
            quadraticFade: 0
            brightness: 0.4
            castsShadow: true
        }
        // Eight that do not, around the floor.
        Repeater3D {
            model: 8
            delegate: PointLight {
                required property int index
                position: Qt.vector3d(800 * Math.cos(index * Math.PI / 4), 150,
                                      800 * Math.sin(index * Math.PI / 4))
                color: Qt.hsva(index / 8, 0.6, 1, 1)
                brightness: 0.3
                quadraticFade: 0
                linearFade: 0.5
            }
        }

        // The floor, under the large texture.
        Model {
            source: "#Rectangle"
            eulerRotation.x: -90
            y: -60
            scale: Qt.vector3d(24, 24, 1)
            receivesShadows: true
            materials: DefaultMaterial {
                diffuseMap: Texture {
                    source: "quick3d/stress-tiles.png"
                    scaleU: 4
                    scaleV: 4
                    tilingModeHorizontal: Texture.Repeat
                    tilingModeVertical: Texture.Repeat
                }
            }
        }

        Texture { id: checker; source: "quick3d/stress-checker.png" }
        Texture { id: rings; source: "quick3d/stress-rings.png" }

        // Five materials taken in turn: plain, textured, metal, textured
        // physically based, and blended.
        DefaultMaterial { id: plain; diffuseColor: "#d0d0d0" }
        DefaultMaterial { id: checkered; diffuseMap: checker }
        PrincipledMaterial { id: metal; baseColor: "#c08040"; metalness: 1; roughness: 0.3 }
        PrincipledMaterial { id: ringed; baseColorMap: rings; roughness: 0.6 }
        PrincipledMaterial { id: glass; baseColor: "#80c0ff"; opacity: 0.5; alphaMode: PrincipledMaterial.Blend }

        // 480 models on a grid of 24 by 20, each mesh and material in turn.
        Repeater3D {
            model: 480
            delegate: Model {
                required property int index
                readonly property int column: index % 24
                readonly property int row: Math.floor(index / 24)
                source: root.meshes[index % 5]
                position: Qt.vector3d((column - 11.5) * 70, (index % 3) * 20, (row - 9.5) * 70)
                eulerRotation: Qt.vector3d(index * 7 % 360, index * 13 % 360, 0)
                scale: Qt.vector3d(0.4, 0.4, 0.4)
                castsShadows: true
                receivesShadows: true
                materials: [plain, checkered, metal, ringed, glass][Math.floor(index / 5) % 5]
            }
        }

        // glTF models read while the window opens, about 5000 triangles each.
        Repeater3D {
            model: 3
            delegate: Node {
                required property int index
                position: Qt.vector3d((index - 1) * 600, 250, -900)
                scale: Qt.vector3d(0.6, 0.6, 0.6)
                RuntimeLoader { source: "quick3d/sphere-grid.glb" }
            }
        }
    }
}
