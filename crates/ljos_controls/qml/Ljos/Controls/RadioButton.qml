// Ljós's own style (docs/DIALECT.md §4.16): one choice of several, a
// circle the size of a check box's square, in the input's colour until
// it is chosen, then the accent with a dot in the accent's ink, as a
// checked check box is. The keys bring a ring around it.
import QtQuick
import QtQuick.Templates as T

T.RadioButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: Theme.spacingSmall
    spacing: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    opacity: enabled ? 1 : 0.64

    indicator: Component {
        Rectangle {
            implicitWidth: 16
            implicitHeight: 16
            x: control.text.length > 0
               ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
               : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: width / 2
            color: control.checked ? Theme.accent : Theme.surface
            border.width: 1
            border.color: control.checked ? Theme.focus : Theme.input

            Rectangle {
                objectName: "dot"
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: 6
                height: 6
                radius: width / 2
                color: Theme.accentForeground
                visible: control.checked
            }
            Rectangle {
                objectName: "focusRing"
                x: -3
                y: -3
                width: parent.width + 6
                height: parent.height + 6
                radius: width / 2
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
                visible: control.visualFocus
            }
        }
    }

    contentItem: Component {
        Item {
            property alias text: label.text
            property alias font: label.font
            property alias color: label.color

            implicitWidth: label.implicitWidth + label.gap
            implicitHeight: label.implicitHeight

            Text {
                id: label
                objectName: "label"
                readonly property real gap: control.indicator
                                            ? control.indicator.width + control.spacing : 0
                x: control.mirrored ? 0 : gap
                width: parent.width - gap
                height: parent.height
                text: control.text
                font: control.font
                color: Theme.text
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: control.mirrored ? Text.AlignRight : Text.AlignLeft
            }
        }
    }
}
