// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.RadioButton {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    indicator: Component {
        RadioIndicator {
            x: host.text ? (host.mirrored ? host.width - width - host.rightPadding : host.leftPadding)
                         : host.leftPadding + (host.availableWidth - width) / 2
            y: host.topPadding + (host.availableHeight - height) / 2
            control: host
        }
    }

    contentItem: Component {
        Item {
            property alias text: label.text
            property alias font: label.font
            property alias color: label.color

            implicitWidth: label.implicitWidth + (host.indicator ? host.indicator.width + host.spacing : 0)
            implicitHeight: label.implicitHeight

            Text {
                id: label
                x: host.indicator && !host.mirrored ? host.indicator.width + host.spacing : 0
                width: parent.width - (host.indicator ? host.indicator.width + host.spacing : 0)
                height: parent.height
                text: host.text
                font: host.font
                color: host.palette.windowText
                verticalAlignment: Text.AlignVCenter
                visible: host.text.length > 0
            }
        }
    }
}
