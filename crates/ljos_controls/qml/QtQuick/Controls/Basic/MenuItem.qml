// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
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

    icon.width: 24
    icon.height: 24

    // Only a checked item shows a mark; one that can be checked keeps the
    // room for it.
    indicator: Item {
        implicitWidth: 28
        implicitHeight: 28
        x: control.mirrored ? control.width - width - control.rightPadding : control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        visible: control.checked

        // The mark is a filled square: a neutral drawing of our own.
        Rectangle {
            x: (parent.width - width) / 2
            y: (parent.height - height) / 2
            width: 14
            height: 14
            radius: 2
            color: control.palette.windowText
        }
    }

    // Only an item that opens a menu points at it: two strokes that meet,
    // a neutral drawing of our own.
    arrow: Item {
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
            color: control.palette.windowText
        }

        Rectangle {
            x: 1
            y: 11
            width: 9
            height: 2
            rotation: control.mirrored ? 45 : -45
            color: control.palette.windowText
        }
    }

    // The text, with room for the indicator on one side and for the arrow
    // on the other.
    // TODO(image): the icon goes beside the text, as `display` says.
    contentItem: Item {
        // What an application reads off the content item of a button.
        property alias text: label.text
        property alias font: label.font
        property alias color: label.color

        implicitWidth: label.implicitWidth + label.before + label.after
        implicitHeight: label.implicitHeight

        Text {
            id: label
            readonly property real before: control.checkable && control.indicator
                                           ? control.indicator.width + control.spacing : 0
            readonly property real after: control.subMenu !== null && control.arrow
                                          ? control.arrow.width + control.spacing : 0
            x: control.mirrored ? after : before
            width: parent.width - before - after
            height: parent.height
            text: control.text
            font: control.font
            color: control.palette.windowText
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
    }

    // A line of the menu, inside the menu's border.
    background: Rectangle {
        implicitWidth: 200
        implicitHeight: 40
        x: 1
        y: 1
        width: control.width - 2
        height: control.height - 2
        color: control.down ? control.palette.midlight
                            : control.highlighted ? control.palette.light : "transparent"
    }
}
