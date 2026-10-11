// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.DialogButtonBox {
    id: control

    // The only button of a box takes half of it, so the box asks for
    // twice what the button does.
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            (control.count === 1 ? implicitContentWidth * 2 : implicitContentWidth)
                            + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    spacing: 1
    padding: 12
    alignment: count === 1 ? Qt.AlignRight : 0

    delegate: Button {}

    background: Component {
        Rectangle {
            implicitHeight: 40
            x: 1
            y: 1
            width: control.width - 2
            height: control.height - 2
            color: control.palette.window
        }
    }
}
