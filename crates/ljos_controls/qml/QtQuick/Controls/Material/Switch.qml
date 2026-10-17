// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.Switch {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    // The indicator has no implicit size: the text and the padding make
    // the height, and a switch is never shorter than its track.
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding, 32)

    padding: 8
    spacing: 8

    indicator: Component {
        Rectangle {
            readonly property bool dark: control.Material.theme === Material.Dark
            // In the dark theme the accent is drawn lighter and the handle
            // on it darker, so the handle stands out from the track.
            readonly property color onTrack: dark ? Qt.tint(control.Material.accent, "#66ffffff")
                                                  : control.Material.accent
            readonly property color offLine: dark ? "#bdbdbd" : "#757575"

            width: 52
            height: 32
            x: control.text ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
                            : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: 16
            color: !control.enabled ? (dark ? "#1ee6e1e5" : "#1e1c1b1f")
                 : control.checked ? onTrack
                 : dark ? "transparent" : "#e7e0ec"
            border.width: 2
            border.color: !control.enabled ? color : control.checked ? onTrack : offLine

            // A 28 pixel disc, drawn smaller when off and larger when on.
            Rectangle {
                x: Math.max(2, Math.min(parent.width - width - 2, control.visualPosition * parent.width - (width / 2)))
                y: 2
                width: 28
                height: 28
                radius: 14
                scale: control.down ? 1 : control.checked ? 24 / 28 : 16 / 28
                color: !control.enabled ? control.Material.background
                     : control.checked ? (parent.dark ? Qt.darker(control.Material.accent, 1.4) : "#ffffff")
                     : parent.offLine
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
