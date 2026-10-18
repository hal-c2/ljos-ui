// The Fusion style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Fusion's own metrics and palette.
import QtQuick
import QtQuick.Templates as T

T.RadioButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    // A ring with a faint shadow inside its top, and a dark dot when
    // chosen.
    indicator: Component {
        Rectangle {
            implicitWidth: 14
            implicitHeight: 14
            x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                            : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: width / 2
            color: control.down ? Qt.darker(control.palette.light, 1.18) : control.palette.light
            border.color: control.visualFocus ? control.palette.highlight
                                              : Qt.darker(control.palette.window, 1.5)

            Rectangle {
                width: parent.width
                height: parent.height - 1
                radius: width / 2
                color: "transparent"
                border.color: Qt.rgba(0, 0, 0, 18 / 255)
            }

            Rectangle {
                width: parent.width / 2.32
                height: width
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                radius: width / 2
                color: Qt.rgba(control.palette.text.r, control.palette.text.g, control.palette.text.b, 180 / 255)
                border.color: Qt.rgba(control.palette.text.r, control.palette.text.g, control.palette.text.b, 200 / 255)
                visible: control.checked
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
