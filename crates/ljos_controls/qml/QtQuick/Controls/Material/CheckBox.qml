// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.CheckBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 8
    topPadding: 15
    bottomPadding: 15
    spacing: 8

    indicator: Component {
        Rectangle {
            implicitWidth: 18
            implicitHeight: 18
            x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                            : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: 2
            color: control.checkState !== Qt.Unchecked ? control.Material.accent : "transparent"
            border.color: control.checkState !== Qt.Unchecked ? control.Material.accent : control.Material.secondaryTextColor
            border.width: 2

            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: 10
                height: control.checkState === Qt.PartiallyChecked ? 4 : 10
                radius: 1
                color: control.Material.background
                visible: control.checkState !== Qt.Unchecked
            }
        }
    }

    contentItem: Component {
        Item {
            property alias text: label.text
            property alias font: label.font
            property alias color: label.color

            implicitWidth: label.implicitWidth + (control.indicator ? control.indicator.width + control.spacing : 0)
            implicitHeight: label.implicitHeight

            Text {
                id: label
                x: control.indicator && !control.mirrored ? control.indicator.width + control.spacing : 0
                width: parent.width - (control.indicator ? control.indicator.width + control.spacing : 0)
                height: parent.height
                text: control.text
                font: control.font
                color: control.Material.foreground
                verticalAlignment: Text.AlignVCenter
                visible: control.text.length > 0
            }
        }
    }
}
