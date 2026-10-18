// The Fusion style of Ljós (CLEAN_ROOM.md), a drawing of our own fitted to
// what qml6 shows for the default palette.
import QtQuick
import QtQuick.Templates as T

T.Dial {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // The dial's square side, which its parts are measured by.
    readonly property real side: Math.min(width, height)

    // A raised disc four fifths of the side across, in the middle, lit
    // from above and casting a shadow down and to the right. Not enabled,
    // it is flat and casts none.
    background: Component {
        Rectangle {
            id: disc

            readonly property color button: control.palette.button

            function shade(scale) {
                return Qt.rgba(button.r * scale, button.g * scale, button.b * scale, button.a)
            }

            implicitWidth: 100
            implicitHeight: 100
            width: Math.round(control.side * 0.8)
            height: width
            x: (control.width - width) / 2
            y: (control.height - height) / 2
            radius: width / 2
            color: control.palette.base
            gradient: control.enabled ? lit : null
            border.color: control.visualFocus ? Qt.darker(control.palette.highlight, 1.25)
                                              : shade(0.545)

            Gradient {
                id: lit
                GradientStop { position: 0; color: Qt.lighter(disc.button, 1.07) }
                GradientStop { position: 0.61; color: disc.button }
                GradientStop { position: 0.62; color: disc.shade(0.979) }
                GradientStop { position: 1; color: disc.shade(0.92) }
            }

            Rectangle {
                x: 1
                y: 1
                z: -1
                width: parent.width
                height: parent.height
                radius: width / 2
                visible: control.enabled
                color: disc.shade(0.866)
            }
        }
    }

    // A small knob near the disc's rim, at the dial's angle. qml6 shades
    // it so faintly that a flat grey saves the four objects a gradient
    // takes.
    handle: Component {
        Rectangle {
            readonly property real reach: control.side * 0.28
            readonly property real turn: control.angle * Math.PI / 180

            width: control.side / 7
            height: width
            x: control.width / 2 + Math.sin(turn) * reach - width / 2
            y: control.height / 2 - Math.cos(turn) * reach - height / 2
            radius: width / 2
            color: Qt.darker(control.palette.button, 1.085)
            border.color: control.visualFocus ? Qt.darker(control.palette.highlight, 1.25)
                                              : control.palette.mid
        }
    }
}
