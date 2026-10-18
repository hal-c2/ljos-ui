// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.DelayButton {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6

    // The text is dark on the button and light on the fill: each is cut
    // off where the fill's edge is, so the edge runs through a letter.
    contentItem: Component {
        Item {
            // What an application reads off the content item of a button.
            property alias text: dark.text
            property alias font: dark.font
            property alias color: dark.color

            // How much of the content item the fill covers, from its leading
            // side.
            readonly property real edge: Math.max(0, Math.min(width, host.progress * host.width
                                                               - (host.mirrored ? host.width - x - width : x)))

            implicitWidth: dark.implicitWidth
            implicitHeight: dark.implicitHeight

            Item {
                x: host.mirrored ? 0 : parent.edge
                width: parent.width - parent.edge
                height: parent.height
                clip: host.progress > 0
                visible: !host.checked && host.progress < 1

                Text {
                    id: dark
                    x: -parent.x
                    width: parent.parent.width
                    height: parent.height
                    text: host.text
                    font: host.font
                    color: host.palette.buttonText
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }

            Item {
                x: host.mirrored ? parent.width - width : 0
                width: parent.edge
                height: parent.height
                clip: true
                visible: host.progress > 0

                Text {
                    x: -parent.x
                    width: parent.parent.width
                    height: parent.height
                    text: host.text
                    font: host.font
                    color: host.palette.highlightedText
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
        }
    }

    // Pressed in once it has gone off; the fill grows from the leading
    // edge.
    background: Component {
        ButtonPanel {
            implicitWidth: 80
            implicitHeight: 24
            control: host
            pressed: host.down || host.checked

            Rectangle {
                x: host.mirrored ? parent.width - width : 0
                width: host.progress * parent.width
                height: parent.height
                radius: 2
                border.color: Qt.darker(host.palette.highlight, 1.4)
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.lighter(host.palette.highlight, 1.2) }
                    GradientStop { position: 1; color: host.palette.highlight }
                }
            }
        }
    }
}
