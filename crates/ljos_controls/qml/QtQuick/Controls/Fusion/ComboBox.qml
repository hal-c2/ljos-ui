// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Controls.Fusion
import QtQuick.Templates as T

T.ComboBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             Math.max(implicitContentHeight, implicitIndicatorHeight)
                             + topPadding + bottomPadding)

    // The arrow has a strip of its own, whichever side it is on.
    leftPadding: padding + (!control.mirrored || !indicator || !indicator.visible ? 0 : 20)
    rightPadding: padding + (control.mirrored || !indicator || !indicator.visible ? 0 : 20)

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
            implicitWidth: 10
            implicitHeight: 6
            x: control.mirrored ? control.padding + 5 : control.width - control.rightPadding
            y: control.topPadding + (control.availableHeight - height) / 2

            Rectangle {
                x: 0
                y: 2
                width: 6
                height: 1.5
                rotation: 45
                color: control.palette.buttonText
                opacity: control.enabled ? 1 : 0.3
            }

            Rectangle {
                x: 4
                y: 2
                width: 6
                height: 1.5
                rotation: -45
                color: control.palette.buttonText
                opacity: control.enabled ? 1 : 0.3
            }
        }
    }

    // A field in either case, as on Qt: one that cannot be typed in shows
    // the text of the current row and leaves a press to the combo box.
    contentItem: Component {
        TextField {
            id: field

            leftPadding: control.mirrored ? 1 : 4
            rightPadding: control.mirrored ? 4 : 1
            topPadding: 4 - control.padding
            bottomPadding: 4 - control.padding

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
                    radius: 2
                    border.color: field.activeFocus ? control.palette.highlight
                                                    : Qt.darker(control.palette.button, 1.4)
                    color: control.palette.base
                }
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 120
            implicitHeight: 24
            visible: !control.flat || control.down
            radius: 2
            readonly property color button: control.palette.button
            color: control.down ? Qt.rgba(button.r * 0.883, button.g * 0.883, button.b * 0.883, button.a)
                                : Qt.darker(button, 1.03)
            gradient: Gradient {
                GradientStop { position: 0; color: control.down ? parent.color : Qt.lighter(control.palette.button, 1.07) }
                GradientStop { position: 1; color: control.down ? parent.color : Qt.darker(control.palette.button, 1.011) }
            }
            border.color: control.visualFocus ? control.palette.highlight
                        : control.enabled ? Qt.darker(control.palette.button, 1.4)
                        : Qt.darker(control.palette.button, 1.22)
        }
    }

    // The rows, under the combo box and as wide as it is, and no taller
    // than the window has room for between the margins: more rows than
    // that scroll.
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

            // The rows are those of `delegateModel`, and the view's
            // current one is the one the keys or the pointer are on.
            contentItem: Component {
                ListView {
                    clip: true
                    implicitHeight: contentHeight
                    model: control.delegateModel
                    currentIndex: control.highlightedIndex
                    highlightMoveDuration: 0

                    ScrollIndicator.vertical: ScrollIndicator {}
                }
            }

            background: Component {
                Rectangle {
                    color: control.palette.base
                    border.color: Qt.darker(control.palette.window, 1.4)
                    radius: 2
                }
            }
        }
    }
}
