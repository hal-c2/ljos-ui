// Ljós's own style (docs/DIALECT.md §4.16): a small box, filled with the
// accent and marked when checked.
import QtQuick
import QtQuick.Templates as T

T.CheckBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding,
                             Theme.controlHeight)

    padding: 0
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
            radius: Theme.radiusSmall
            readonly property bool marked: control.checkState !== Qt.Unchecked
            color: marked ? Theme.accent : Theme.surfaceRaised
            border.width: 1
            border.color: marked ? Theme.accent : Theme.input

            Icon {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                visible: parent.marked
                name: control.checkState === Qt.PartiallyChecked ? "minus" : "check"
                size: Theme.iconSizeSmall
                color: Theme.accentForeground
            }
            // Outside the part, against the canvas: a part filled with the
            // accent would hide a ring drawn on its own edge.
            Rectangle {
                objectName: "focusRing"
                x: -3
                y: -3
                width: parent.width + 6
                height: parent.height + 6
                radius: parent.radius + 3
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
            }
        }
    }
}
