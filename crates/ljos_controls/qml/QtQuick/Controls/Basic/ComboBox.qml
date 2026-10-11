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
    // set in a heavier face (weight 600 on qml6).
    delegate: ItemDelegate {
        width: control.width
        text: control.textAt(index)
        highlighted: control.highlightedIndex === index
        hoverEnabled: control.hoverEnabled
        font.weight: control.currentIndex === index ? Font.DemiBold : Font.Normal
    }

    // Two strokes that meet below: a neutral drawing of our own.
    indicator: Component {
        Item {
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
    }

    // A field in either case, as on Qt: one that cannot be typed in shows
    // the text of the current row and leaves a press to the combo box.
    contentItem: Component {
        TextField {
            id: field

            leftPadding: control.mirrored ? 1 : 12
            rightPadding: control.mirrored ? 12 : 1
            topPadding: 6 - control.padding
            bottomPadding: 6 - control.padding

            text: control.editable ? control.editText : control.displayText

            enabled: control.editable
            autoScroll: control.editable
            // The keys move through the rows while the popup is open.
            readOnly: control.down
            inputMethodHints: control.inputMethodHints
            validator: control.validator
            selectByMouse: control.selectTextByMouse

            font: control.font
            color: control.editable ? control.palette.text : control.palette.buttonText
            selectionColor: control.palette.highlight
            selectedTextColor: control.palette.highlightedText
            verticalAlignment: Text.AlignVCenter

            // The combo box speaks for its text.
            Accessible.ignored: !control.editable
            Accessible.name: control.Accessible.name

            background: Component {
                Rectangle {
                    visible: control.enabled && control.editable && !control.flat
                    border.width: field.activeFocus ? 2 : 1
                    border.color: field.activeFocus ? control.palette.highlight
                                                    : control.palette.button
                    color: control.palette.base
                }
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 140
            implicitHeight: 40
            color: control.down ? control.palette.mid : control.palette.button
            // Where the keyboard is, when a key brought it there.
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0
            visible: !control.flat || control.down
        }
    }

    // The rows, under the combo box and as wide as it is, and no taller
    // than the window has room for between the margins: more rows than
    // that scroll. TODO(listview): a list view on `delegateModel`.
    popup: Component {
        T.Popup {
            y: control.height
            width: control.width
            implicitHeight: contentItem.implicitHeight
            topMargin: 6
            bottomMargin: 6
            // Not on any press outside it: a press on the combo box is the
            // combo box's to answer.
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent

            // The combo box puts its rows in what this scrolls, and says
            // how high they are together.
            contentItem: Component {
                Flickable {
                    implicitHeight: contentHeight
                    clip: true

                    ScrollIndicator.vertical: ScrollIndicator {}
                }
            }

            background: Component {
                Rectangle {
                    color: control.palette.window
                    border.color: control.palette.mid
                }
            }
        }
    }
}
