// The box of Fusion's check boxes, a drawing of our own (CLEAN_ROOM.md):
// square, with a faint shadow along the inside of its top, a tick when
// checked and a shaded square when partly checked.
import QtQuick
import QtQuick.Shapes
import QtQuick.Templates as T
import QtQuick.Controls.Fusion

Rectangle {
    id: indicator

    property T.AbstractButton control
    readonly property color pressedColor: Qt.darker(control.palette.light, 1.18)
    readonly property color checkMarkColor: control.enabled
        ? control.palette.text
        : Qt.rgba(control.palette.mid.r * 0.86, control.palette.mid.g * 0.86, control.palette.mid.b * 0.86, 1)
    // A check box's state; anything else that checks says only whether.
    property int checkState: control.checked ? Qt.Checked : Qt.Unchecked

    function ink(alpha) {
        return Qt.rgba(checkMarkColor.r, checkMarkColor.g, checkMarkColor.b, alpha / 255)
    }

    implicitWidth: 14
    implicitHeight: 14

    color: control.down ? pressedColor : control.palette.base
    border.color: control.visualFocus ? Qt.darker(control.palette.highlight, 1.25)
                                      : Qt.darker(control.palette.window, 1.27)

    Rectangle {
        x: 1
        y: 1
        width: parent.width - 2
        height: 1
        color: Fusion.topShadow
        visible: indicator.control.enabled && !indicator.control.down
    }

    // A short stroke down and a long one up, as qml6's tick lies.
    Shape {
        width: 14
        height: 14
        x: (parent.width - width) / 2
        y: (parent.height - height) / 2
        visible: indicator.checkState === Qt.Checked
        ShapePath {
            strokeColor: indicator.ink(210)
            strokeWidth: 1.8
            fillColor: "transparent"
            capStyle: ShapePath.FlatCap
            joinStyle: ShapePath.MiterJoin
            PathSvg { path: "M 4.6 7 L 6.8 10.6 L 9.8 3" }
        }
    }

    Rectangle {
        x: (parent.width - width) / 2
        y: (parent.height - height) / 2
        width: 8
        height: 8
        visible: indicator.checkState === Qt.PartiallyChecked
        border.color: indicator.ink(180)
        gradient: Gradient {
            GradientStop { position: 0; color: indicator.ink(80) }
            GradientStop { position: 1; color: indicator.ink(140) }
        }
    }
}
