// The Material style of Ljós (CLEAN_ROOM.md): a ring with a dot in it,
// under Material's metrics and colours, as a running Qt reports them.
import QtQuick
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.RadioButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 8
    verticalPadding: 14
    spacing: 8

    indicator: Component {
        Rectangle {
            readonly property color ink: !control.enabled ? control.Material.hintTextColor
                                       : control.checked ? control.Material.accent
                                       : control.Material.secondaryTextColor
            implicitWidth: 20
            implicitHeight: 20
            x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                            : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: width / 2
            color: "transparent"
            border.width: 2
            border.color: ink

            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: 10
                height: 10
                radius: width / 2
                color: parent.ink
                scale: control.checked ? 1 : 0
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
                color: control.enabled ? control.Material.foreground : control.Material.hintTextColor
                verticalAlignment: Text.AlignVCenter
                visible: control.text.length > 0
            }
        }
    }
}
