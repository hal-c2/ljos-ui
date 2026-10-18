// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.MenuItem {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 16
    icon.height: 16

    // Only a checked item shows a mark; one that can be checked keeps the
    // room for it.
    indicator: Component {
        Item {
            implicitWidth: 14
            implicitHeight: 14
            x: control.mirrored ? control.width - width - control.rightPadding : control.leftPadding
            y: control.topPadding + (control.availableHeight - height) / 2
            visible: control.checked

            // The mark is a filled square: a neutral drawing of our own.
            Rectangle {
                x: 3
                y: 3
                width: 8
                height: 8
                radius: 1
                color: control.highlighted ? control.palette.highlightedText : control.palette.text
            }
        }
    }

    // Only an item that opens a menu points at it: two strokes that meet,
    // a neutral drawing of our own.
    arrow: Component {
        Item {
            implicitWidth: 11
            implicitHeight: 18
            x: control.mirrored ? control.padding : control.width - width - control.padding
            y: control.topPadding + (control.availableHeight - height) / 2
            visible: control.subMenu !== null

            Rectangle {
                x: 1
                y: 5
                width: 9
                height: 2
                rotation: control.mirrored ? -45 : 45
                color: control.highlighted ? control.palette.highlightedText : control.palette.text
            }

            Rectangle {
                x: 1
                y: 11
                width: 9
                height: 2
                rotation: control.mirrored ? 45 : -45
                color: control.highlighted ? control.palette.highlightedText : control.palette.text
            }
        }
    }

    // The icon and the text, with room for the indicator on one side and
    // for the arrow on the other.
    contentItem: Component {
        IconLabel {
            readonly property real before: control.checkable && control.indicator
                                           ? control.indicator.width + control.spacing : 0
            readonly property real after: control.subMenu !== null && control.arrow
                                          ? control.arrow.width + control.spacing : 0
            leftPadding: control.mirrored ? after : before
            rightPadding: control.mirrored ? before : after
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            alignment: Qt.AlignLeft
            text: control.text
            font: control.font
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
        }
    }

    // Shown only for the highlighted item.
    background: Component {
        Rectangle {
            implicitWidth: 200
            implicitHeight: 20
            color: control.palette.highlight
            visible: control.highlighted
        }
    }
}
