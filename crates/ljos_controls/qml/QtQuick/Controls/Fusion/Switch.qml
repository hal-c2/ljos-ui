// The Fusion style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Fusion's own metrics and palette.
import QtQuick
import QtQuick.Templates as T

T.Switch {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    indicator: Component {
        Rectangle {
            implicitWidth: 40
            implicitHeight: 16
            x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                            : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: 8
            color: control.checked ? control.palette.highlight : control.palette.button
            border.width: 1
            border.color: control.palette.mid

            Rectangle {
                x: Math.max(0, Math.min(parent.width - width, control.visualPosition * parent.width - (width / 2)))
                width: 20
                height: 16
                radius: 8
                color: control.palette.button
                border.color: control.palette.mid
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
                color: control.palette.windowText
                verticalAlignment: Text.AlignVCenter
                visible: control.text.length > 0
            }
        }
    }
}
