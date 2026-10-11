// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
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

    padding: 12

    background: Component {
        Rectangle {
            color: control.palette.window
            border.color: control.palette.dark
        }
    }

    // TODO(text): a Label with `padding: 12` and its own background, once
    // Text has padding. Until then the title sits in an item that has the
    // label's `text` and `font`.
    header: Component {
        Item {
            property alias text: label.text
            property alias font: label.font

            visible: control.title.length > 0
            implicitWidth: label.implicitWidth + 24
            implicitHeight: label.implicitHeight + 24

            Rectangle {
                x: 1
                y: 1
                width: parent.width - 2
                height: parent.height - 1
                color: control.palette.window
            }

            Label {
                id: label
                x: 12
                y: 12
                width: parent.width - 24
                text: control.title
                font.bold: true
                elide: Text.ElideRight
            }
        }
    }

    footer: Component {
        DialogButtonBox {
            visible: count > 0
        }
    }
}
