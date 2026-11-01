import QtQuick
import QtQuick.Shapes

// A filled and stroked SVG path, in the item's own coordinates.
Shape {
    id: root

    property string path
    property alias fillColor: shape.fillColor
    property alias strokeColor: shape.strokeColor
    property alias strokeWidth: shape.strokeWidth
    property alias strokeStyle: shape.strokeStyle
    property alias joinStyle: shape.joinStyle
    property alias capStyle: shape.capStyle
    property alias dashPattern: shape.dashPattern
    property alias dashOffset: shape.dashOffset
    property alias fillGradient: shape.fillGradient

    ShapePath {
        id: shape
        PathSvg { path: root.path }
    }
}
