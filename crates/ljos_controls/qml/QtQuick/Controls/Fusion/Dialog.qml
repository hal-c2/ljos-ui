// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports. The title and the buttons sit on the dialog with
// nothing behind them.
import QtQuick
import QtQuick.Controls.Fusion
import QtQuick.Templates as T

T.Dialog {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding,
                            implicitHeaderWidth,
                            implicitFooterWidth)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding
                             + (implicitHeaderHeight > 0 ? implicitHeaderHeight + spacing : 0)
                             + (implicitFooterHeight > 0 ? implicitFooterHeight + spacing : 0))

    padding: 6

    background: Component {
        Rectangle {
            color: control.palette.window
            border.color: control.palette.mid
            radius: 2
        }
    }

    header: Component {
        Label {
            text: control.title
            visible: control.title.length > 0
            elide: Label.ElideRight
            font.bold: true
            padding: 6
            background: Rectangle {
                color: "transparent"
            }
        }
    }

    footer: Component {
        DialogButtonBox {
            visible: count > 0
        }
    }
}
