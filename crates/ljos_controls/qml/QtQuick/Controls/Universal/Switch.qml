// The Universal style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Universal's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Templates as T

T.Switch {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 5
    spacing: 8

    indicator: Component {
        Rectangle {
            implicitWidth: 44
            implicitHeight: 20
            x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                            : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: 10
            color: control.checked ? control.Universal.accent : "transparent"
            border.width: 2
            border.color: control.checked ? control.Universal.accent : control.Universal.baseMediumHighColor

            Rectangle {
                x: Math.max(4, Math.min(parent.width - width - 4, control.visualPosition * parent.width - (width / 2)))
                y: (parent.height - height) / 2
                width: 10
                height: 10
                radius: 5
                color: control.checked ? control.Universal.background : control.Universal.baseMediumHighColor
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
                color: control.Universal.foreground
                verticalAlignment: Text.AlignVCenter
                visible: control.text.length > 0
            }
        }
    }
}
