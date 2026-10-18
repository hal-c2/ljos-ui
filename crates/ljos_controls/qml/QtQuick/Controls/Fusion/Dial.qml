// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Templates as T

T.Dial {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // A ring as large as the dial is square, in the middle of it.
    background: Component {
        Rectangle {
            implicitWidth: 100
            implicitHeight: 100
            x: control.width / 2 - width / 2
            y: control.height / 2 - height / 2
            width: Math.max(64, Math.min(control.width, control.height))
            height: width
            radius: width / 2
            color: "transparent"
            border.width: 2
            border.color: control.palette.dark
            opacity: control.enabled ? 1 : 0.3
        }
    }

    // A pointer inside the ring, turned to the angle.
    handle: Component {
        Rectangle {
            readonly property real reach: Math.min(control.width, control.height) / 2 - 2 - height
            readonly property real turn: control.angle * Math.PI / 180

            x: control.width / 2 + Math.sin(turn) * reach - width / 2
            y: control.height / 2 - Math.cos(turn) * reach - height / 2
            implicitWidth: 6
            implicitHeight: 14
            radius: 3
            rotation: control.angle
            color: control.visualFocus ? control.palette.highlight : control.palette.dark
            opacity: control.enabled ? 1 : 0.3
        }
    }
}
