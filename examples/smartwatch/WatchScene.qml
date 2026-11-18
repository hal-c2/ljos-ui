// The watch in 3D. The parts are separate files from assets/watch (made by
// build_watch.py); each sits under a Node at the pivot rig.json gives. The
// screen is a disc drawn here whose texture is the live `face` item, so the
// watch shows the app's own UI. The light strips are emissive, so the
// scene's glow blooms them and the screen's brightest pixels, and nothing
// else.
//
// The scene runs no timers or animations of its own: whoever holds it
// drives `yaw`, `crown` and `flex`. Dragging adds to the turn through
// `dragYaw` and `dragPitch`, so a binding on `yaw` keeps working.
//
// This file stays in the QtQuick3D vocabulary: Ljós runs it as it is.
import QtQuick
import QtQuick3D
import QtQuick3D.Helpers
import QtQuick3D.AssetUtils

View3D {
    id: scene

    // The item whose picture is the screen, square (454 by 454, say). The
    // circle inscribed in it is what shows, with its top at 12 o'clock.
    property Item face: null
    // Degrees the crown is turned about its axis.
    property real crown: 0
    // How far the strap curls in, 0 to 1.
    property real flex: 0
    // How far the button is pressed in, 0 to 1.
    property real press: 0
    // The watch's turn about its 12 o'clock axis and its tilt towards the
    // viewer, in degrees. The camera and the lights stay where they are,
    // so highlights sweep over the case as it turns.
    property real yaw: -24
    property real pitch: 10
    // The camera's distance from the watch's centre, in millimetres.
    property real distance: 175
    property bool bloom: true
    property bool dark: true
    // The edge light round the bezel, the side band and the lug strips.
    property color edgeTint: "#00e5ff"
    // The ring on the crown.
    property color accentTint: "#ff2bd6"
    // How bright the light strips are; 1 is their normal glow.
    property real glowLevel: 1
    // How bright the screen is. Above 1 its brightest pixels pass the
    // glow's threshold and bloom a little; mid tones stay as the UI drew them.
    property real screenLevel: 1.3

    // Dragging turns the watch on top of yaw and pitch.
    property bool interactive: true
    property real dragYaw: 0
    property real dragPitch: 0
    // True while the viewer drags, so an outside turntable can pause.
    readonly property bool dragging: drag.active

    // rig.json is in metres; this scene's unit is the millimetre.
    readonly property real m: 1000
    readonly property vector3d metre: Qt.vector3d(m, m, m)

    // The rig's numbers, in millimetres.
    readonly property real screenZ: 3.4
    readonly property real screenR: 17.8
    readonly property real linkPitch: 6.5
    readonly property real bend: -9 + -11 * scene.flex

    // A flat ring of `n` quads between radii a and b at height z, facing +Z.
    function annulus(a, b, z, n) {
        const pts = []
        for (let i = 0; i <= n; ++i) {
            const t = 2 * Math.PI * i / n
            const c = Math.cos(t), s = Math.sin(t)
            pts.push(Qt.vector3d(a * c, a * s, z))
            pts.push(Qt.vector3d(b * c, b * s, z))
        }
        return pts
    }
    // The screen: a fan round its centre.
    function disc(r, z, n) {
        const pts = [Qt.vector3d(0, 0, z)]
        for (let i = 0; i <= n; ++i) {
            const t = 2 * Math.PI * i / n
            pts.push(Qt.vector3d(r * Math.cos(t), r * Math.sin(t), z))
        }
        return pts
    }
    // Its texture coordinates: the disc inscribed in the unit square, V up,
    // so the item's top edge is at 12 o'clock.
    function discUv(n) {
        const uvs = [Qt.vector2d(0.5, 0.5)]
        for (let i = 0; i <= n; ++i) {
            const t = 2 * Math.PI * i / n
            uvs.push(Qt.vector2d(0.5 + 0.5 * Math.cos(t), 0.5 + 0.5 * Math.sin(t)))
        }
        return uvs
    }
    function glow(c, k) {
        return Qt.vector3d(c.r * k, c.g * k, c.b * k)
    }

    DragHandler {
        id: drag
        target: null
        enabled: scene.interactive
        property real yaw0: 0
        property real pitch0: 0
        onActiveChanged: {
            if (active) {
                yaw0 = scene.dragYaw
                pitch0 = scene.dragPitch
            }
        }
        onTranslationChanged: {
            if (!active)
                return
            scene.dragYaw = yaw0 + translation.x * 0.45
            const p = pitch0 + translation.y * 0.3
            scene.dragPitch = Math.max(-70 - scene.pitch, Math.min(70 - scene.pitch, p))
        }
    }

    environment: ExtendedSceneEnvironment {
        // Whatever is behind the view shows round the watch.
        backgroundMode: SceneEnvironment.Transparent
        antialiasingMode: SceneEnvironment.MSAA
        antialiasingQuality: SceneEnvironment.High
        tonemapMode: SceneEnvironment.TonemapModeNone
        glowEnabled: scene.bloom
        glowStrength: 1.0
        glowIntensity: scene.dark ? 0.4 : 0.25
        glowBloom: 0.05
        // Added on top: the default soft light darkens the lit case too.
        glowBlendMode: ExtendedSceneEnvironment.GlowBlendMode.Additive
        // Only what is well past white glows: the strips and the
        // screen's brightest pixels, never the lit case.
        glowHDRMinimumValue: 1.25
        glowQualityHigh: true
        glowUseBicubicUpscale: true
        glowLevel: ExtendedSceneEnvironment.GlowLevel.One
            | ExtendedSceneEnvironment.GlowLevel.Two
            | ExtendedSceneEnvironment.GlowLevel.Three
    }

    PerspectiveCamera {
        z: scene.distance
        fieldOfView: 30
        clipNear: 5
        clipFar: 2000
    }

    // Key from the upper left, a cool fill from the camera's side and a
    // warm magenta rim from behind, all fixed to the camera.
    DirectionalLight {
        eulerRotation.x: -35
        eulerRotation.y: -35
        brightness: scene.dark ? 2.6 : 1.8
    }
    DirectionalLight {
        eulerRotation.x: -5
        eulerRotation.y: 25
        color: scene.dark ? "#7fb4d8" : "#ffffff"
        brightness: scene.dark ? 1.3 : 0.9
    }
    DirectionalLight {
        eulerRotation.x: -20
        eulerRotation.y: 160
        color: scene.dark ? "#b04aa8" : "#ffffff"
        brightness: scene.dark ? 0.55 : 0.4
    }
    DirectionalLight {
        eulerRotation.x: 60
        color: scene.dark ? "#3a5a7a" : "#ffffff"
        brightness: 0.5
    }

    Node {
        eulerRotation.x: scene.pitch + scene.dragPitch
        Node {
            id: watch
            eulerRotation.y: scene.yaw + scene.dragYaw

            RuntimeLoader {
                source: "assets/watch/case.glb"
                scale: scene.metre
            }
            RuntimeLoader {
                source: "assets/watch/glass.glb"
                scale: scene.metre
            }

            // The screen.
            Model {
                objectName: "screen"
                geometry: ProceduralMesh {
                    primitiveMode: ProceduralMesh.TriangleFan
                    positions: scene.disc(scene.screenR, scene.screenZ, 96)
                    uv0: scene.discUv(96)
                }
                materials: PrincipledMaterial {
                    // Black and rough, so the lights add nothing: the
                    // screen is only its own light.
                    baseColor: "black"
                    metalness: 0
                    roughness: 1
                    specularAmount: 0
                    emissiveMap: Texture {
                        sourceItem: scene.face
                    }
                    emissiveFactor: Qt.vector3d(scene.screenLevel, scene.screenLevel,
                                                scene.screenLevel)
                }
            }

            // The edge light, in the bezel's channel.
            Model {
                pickable: false
                castsShadows: false
                geometry: ProceduralMesh {
                    primitiveMode: ProceduralMesh.TriangleStrip
                    positions: scene.annulus(21.58, 21.92, 4.3, 96)
                }
                materials: PrincipledMaterial {
                    baseColor: "black"
                    cullMode: Material.NoCulling
                    emissiveFactor: scene.glow(scene.edgeTint, 4 * scene.glowLevel)
                }
            }
            // The side band in the case's groove.
            Model {
                source: "#Cylinder"
                pickable: false
                castsShadows: false
                z: -1.8
                eulerRotation.x: 90
                // #Cylinder is 100 across and 100 tall, along Y.
                scale: Qt.vector3d(22.45 / 50, 0.7 / 100, 22.45 / 50)
                materials: PrincipledMaterial {
                    baseColor: "black"
                    emissiveFactor: scene.glow(scene.edgeTint, 2.5 * scene.glowLevel)
                }
            }
            Strip {
                position: Qt.vector3d(14.83, 19.6, -1.4)
                size: Qt.vector3d(0.08, 6.4, 0.32)
                tint: scene.edgeTint
                level: scene.glowLevel
            }
            Strip {
                position: Qt.vector3d(-14.83, 19.6, -1.4)
                size: Qt.vector3d(0.08, 6.4, 0.32)
                tint: scene.edgeTint
                level: scene.glowLevel
            }
            Strip {
                position: Qt.vector3d(14.83, -19.6, -1.4)
                size: Qt.vector3d(0.08, 6.4, 0.32)
                tint: scene.edgeTint
                level: scene.glowLevel
            }
            Strip {
                position: Qt.vector3d(-14.83, -19.6, -1.4)
                size: Qt.vector3d(0.08, 6.4, 0.32)
                tint: scene.edgeTint
                level: scene.glowLevel
            }

            // The crown turns about its own X.
            Node {
                position: Qt.vector3d(22.6, 0, -0.3)
                eulerRotation.x: scene.crown
                RuntimeLoader {
                    source: "assets/watch/crown.glb"
                    scale: scene.metre
                }
                Model {
                    source: "#Cylinder"
                    pickable: false
                    castsShadows: false
                    x: 1.7
                    eulerRotation.z: 90
                    scale: Qt.vector3d(3.0 / 50, 0.25 / 100, 3.0 / 50)
                    materials: PrincipledMaterial {
                        baseColor: "black"
                        emissiveFactor: scene.glow(scene.accentTint, 4 * scene.glowLevel)
                    }
                }
            }
            // The button slides in along its press axis.
            Node {
                position: Qt.vector3d(19.53 - 0.866 * 0.6 * scene.press,
                                      -11.27 + 0.5 * 0.6 * scene.press, -0.3)
                RuntimeLoader {
                    source: "assets/watch/button.glb"
                    scale: scene.metre
                }
            }

            // The straps: seven links each, every one hinged on the one
            // before and bent by the same angle, so the strap curls back.
            // The bottom strap is the top one turned half round.
            Repeater3D {
                model: 2
                delegate: Node {
                    required property int index
                    position: Qt.vector3d(0, index === 0 ? 24 : -24, -1.2)
                    eulerRotation.z: index === 0 ? 0 : 180
                    Node {
                        eulerRotation.x: scene.bend
                        RuntimeLoader { source: "assets/watch/link.glb"; scale: scene.metre }
                        Node {
                            y: scene.linkPitch
                            eulerRotation.x: scene.bend
                            RuntimeLoader { source: "assets/watch/link.glb"; scale: scene.metre }
                            Node {
                                y: scene.linkPitch
                                eulerRotation.x: scene.bend
                                RuntimeLoader { source: "assets/watch/link.glb"; scale: scene.metre }
                                Node {
                                    y: scene.linkPitch
                                    eulerRotation.x: scene.bend
                                    RuntimeLoader { source: "assets/watch/link.glb"; scale: scene.metre }
                                    Node {
                                        y: scene.linkPitch
                                        eulerRotation.x: scene.bend
                                        RuntimeLoader { source: "assets/watch/link.glb"; scale: scene.metre }
                                        Node {
                                            y: scene.linkPitch
                                            eulerRotation.x: scene.bend
                                            RuntimeLoader { source: "assets/watch/link.glb"; scale: scene.metre }
                                            Node {
                                                y: scene.linkPitch
                                                eulerRotation.x: scene.bend
                                                RuntimeLoader { source: "assets/watch/link.glb"; scale: scene.metre }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
