// One light strip of the car: a thin box that gives off light, so the
// scene's glow blooms it. `size` is in the car's metres; `level` above 1
// makes it brighter than white, which is what the glow picks up.
import QtQuick
import QtQuick3D

Model {
    id: strip

    property vector3d size: Qt.vector3d(1, 1, 1)
    property color tint: "#00e5ff"
    property real level: 1

    source: "#Cube"
    // #Cube is 100 units across.
    scale: Qt.vector3d(strip.size.x / 100, strip.size.y / 100, strip.size.z / 100)
    pickable: false
    castsShadows: false
    materials: PrincipledMaterial {
        baseColor: "black"
        emissiveFactor: Qt.vector3d(strip.tint.r * 4 * strip.level,
                                    strip.tint.g * 4 * strip.level,
                                    strip.tint.b * 4 * strip.level)
    }
}
