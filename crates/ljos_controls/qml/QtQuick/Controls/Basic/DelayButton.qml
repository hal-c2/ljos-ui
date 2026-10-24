// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.DelayButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    horizontalPadding: padding + 2

    contentItem: Component {
        Text {
            text: control.text
            font: control.font
            color: control.checked ? control.palette.brightText : control.palette.buttonText
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
    }

    // The button fills from its leading edge as the progress grows.
    background: Component {
        Rectangle {
            implicitWidth: 100
            implicitHeight: 40
            // Held down, it and its fill are half way to `mid`, as a
            // button is (basic-states.feature).
            function held(c) {
                const mid = control.palette.mid
                return Qt.rgba(Math.round((c.r + mid.r) * 127.5 + 0.001) / 255,
                               Math.round((c.g + mid.g) * 127.5 + 0.001) / 255,
                               Math.round((c.b + mid.b) * 127.5 + 0.001) / 255, 1)
            }

            color: control.down ? held(control.palette.button) : control.palette.button
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0

            Rectangle {
                width: control.progress * parent.width
                height: parent.height
                x: control.mirrored ? parent.width - width : 0
                color: control.down ? parent.held(control.palette.dark) : control.palette.dark
            }
        }
    }
}
