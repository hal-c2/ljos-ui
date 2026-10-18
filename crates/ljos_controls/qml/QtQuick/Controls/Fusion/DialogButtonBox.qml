// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
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

    spacing: 6
    padding: 6
    alignment: count === 1 ? Qt.AlignRight : 0

    delegate: Button {}

    background: Component {
        Rectangle {
            color: "transparent"
        }
    }
}
