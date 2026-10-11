// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.ComboBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             Math.max(implicitContentHeight, implicitIndicatorHeight)
                             + topPadding + bottomPadding)

    // The text keeps clear of the indicator, whichever side it is on.
    leftPadding: padding + (!control.mirrored || !indicator || !indicator.visible
                            ? 0 : indicator.width + spacing)
    rightPadding: padding + (control.mirrored || !indicator || !indicator.visible
                             ? 0 : indicator.width + spacing)

    // A row of the popup: its text is the row's, and the current row is
    // set in a heavier face.
    delegate: ItemDelegate {
        width: control.width
        text: control.textAt(index)
        highlighted: control.highlightedIndex === index
        hoverEnabled: control.hoverEnabled
        font.bold: control.currentIndex === index
    }

    // Two strokes that meet below: a neutral drawing of our own.
    indicator: Item {
        implicitWidth: 40
        implicitHeight: 28
        x: control.mirrored ? control.padding : control.width - width - control.padding
        y: control.topPadding + (control.availableHeight - height) / 2

        Rectangle {
            x: 12
            y: 13
            width: 10
            height: 2
            rotation: 45
            color: control.palette.dark
            opacity: control.enabled ? 1 : 0.3
        }

        Rectangle {
            x: 18
            y: 13
            width: 10
            height: 2
            rotation: -45
            color: control.palette.dark
            opacity: control.enabled ? 1 : 0.3
        }
    }

    // TODO(text-input): a field to type in when the combo box is editable.
    contentItem: Item {
        // What an application reads off the content item.
        property alias text: label.text
        property alias font: label.font
        property alias color: label.color

        implicitHeight: label.implicitHeight + 12

        Text {
            id: label
            x: control.mirrored ? 0 : 12
            width: parent.width - 12
            height: parent.height
            text: control.displayText
            font: control.font
            color: control.palette.buttonText
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: control.mirrored ? Text.AlignRight : Text.AlignLeft
        }
    }

    background: Rectangle {
        implicitWidth: 140
        implicitHeight: 40
        color: control.down ? control.palette.mid : control.palette.button
        visible: !control.flat || control.down
    }

    // The rows, under the combo box and as wide as it is.
    // TODO(listview): a list that scrolls when there are more rows than
    // fit in the window.
    popup: T.Popup {
        y: control.height
        width: control.width
        height: contentItem.implicitHeight
        topMargin: 6
        bottomMargin: 6
        // Not on any press outside it: a press on the combo box is the
        // combo box's to answer.
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent

        contentItem: Column {}

        background: Rectangle {
            color: control.palette.window
            border.color: control.palette.mid
        }
    }
}
