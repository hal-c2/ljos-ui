import Ljos.Ui
import "logic/glyphs.ts" as Glyphs

// One of the cockpit's glyphs (logic/glyphs.ts), stroked at any size.
Item {
    id: icon

    property Glyphs.Mark glyph: ""
    property color color: "white"
    // The stroke on the 24-unit grid; it scales with the icon.
    property real weight: 1.8

    implicitWidth: 24
    implicitHeight: 24

    Shape {
        anchors.centerIn: parent
        width: 24
        height: 24
        scale: Math.min(icon.width, icon.height) / 24
        ShapePath {
            strokeColor: icon.color
            strokeWidth: icon.weight
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg {
                path: Glyphs.path(icon.glyph)
            }
        }
    }
}
