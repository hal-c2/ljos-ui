// The Universal style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Universal's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Universal
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
            color: control.Universal.background
            border.color: control.Universal.chromeHighColor
        }
    }

    header: Component {
        Item {
            property alias text: label.text
            property alias font: label.font

            visible: control.title.length > 0
            implicitWidth: label.implicitWidth + 24
            implicitHeight: label.implicitHeight + 24

            Label {
                id: label
                x: 12
                y: 12
                width: parent.width - 24
                text: control.title
                font.bold: true
                color: control.Universal.foreground
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
