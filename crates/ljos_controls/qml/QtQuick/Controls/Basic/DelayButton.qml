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
            color: control.down ? control.palette.mid : control.palette.button
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0

            Rectangle {
                width: control.progress * parent.width
                height: parent.height
                x: control.mirrored ? parent.width - width : 0
                color: control.palette.dark
            }
        }
    }
}
