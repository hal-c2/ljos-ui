// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Material
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

    padding: 24

    background: Component {
        Rectangle {
            radius: 4
            color: control.Material.dialogColor
            border.color: control.Material.dividerColor
        }
    }

    header: Component {
        Item {
            property alias text: label.text
            property alias font: label.font

            visible: control.title.length > 0
            implicitWidth: label.implicitWidth + 48
            implicitHeight: label.implicitHeight + 24

            Label {
                id: label
                x: 24
                y: 12
                width: parent.width - 48
                text: control.title
                font.bold: true
                color: control.Material.foreground
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
