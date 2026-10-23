// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.Button {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    topInset: 6
    bottomInset: 6
    topPadding: 14
    bottomPadding: 14
    leftPadding: 24
    rightPadding: 16
    spacing: 8

    icon.width: 24
    icon.height: 24

    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            text: control.text
            font: control.font
            color: control.Material.foreground
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 64
            implicitHeight: 40
            radius: 4
            visible: !control.flat || control.down || control.checked || control.highlighted
                     || pressLayer.opacity > 0
            color: control.Material.buttonColor(
                control.Material.theme,
                control.Material.background,
                control.Material.accent,
                control.enabled,
                control.flat,
                control.highlighted,
                control.checked
            )
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0

            // The shadow that lifts it off the page, a step higher while it
            // is held. Layers of faint black under the background stand in
            // for a blur.
            Rectangle {
                z: -1
                visible: !control.flat && control.enabled
                y: 1; width: parent.width; height: parent.height
                radius: parent.radius
                color: "#24000000"
            }
            Rectangle {
                z: -1
                visible: !control.flat && control.enabled
                x: -1; y: 1; width: parent.width + 2; height: parent.height + 2
                radius: parent.radius + 1
                color: "#12000000"
            }
            Rectangle {
                z: -1
                visible: !control.flat && control.enabled && control.down
                x: -2; y: 2; width: parent.width + 4; height: parent.height + 5
                radius: parent.radius + 2
                color: "#10000000"
            }
            Rectangle {
                z: -1
                visible: !control.flat && control.enabled && control.down
                x: -3; y: 3; width: parent.width + 6; height: parent.height + 9
                radius: parent.radius + 3
                color: "#0a000000"
            }

            // The layer a press lays over it, of the theme's text colour:
            // it comes in over a tenth of a second and fades out slower.
            Rectangle {
                id: pressLayer
                width: parent.width; height: parent.height
                radius: parent.radius
                color: control.Material.theme === Material.Dark ? "#1fffffff" : "#10000000"
                opacity: control.down ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: control.down ? 100 : 400 } }
            }
        }
    }
}
