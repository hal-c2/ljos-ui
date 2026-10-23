// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.Fusion
import QtQuick.Controls.Fusion.impl
import QtQuick.Shapes
import QtQuick.Templates as T

T.ComboBox {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             Math.max(implicitContentHeight, implicitIndicatorHeight)
                             + topPadding + bottomPadding)

    // The arrow has a strip of its own, whichever side it is on.
    leftPadding: padding + (!host.mirrored || !indicator || !indicator.visible ? 0 : 20)
    rightPadding: padding + (host.mirrored || !indicator || !indicator.visible ? 0 : 20)

    // A row of the popup: its text is the row's, and the current row is
    // set in a heavier face (weight 600 on qml6).
    delegate: ItemDelegate {
        width: host.width
        text: host.textAt(index)
        highlighted: host.highlightedIndex === index
        hoverEnabled: host.hoverEnabled
        font.weight: host.currentIndex === index ? Font.DemiBold : Font.Normal
    }

    // In a strip of its own, as wide as the padding leaves it.
    indicator: Component {
        Item {
            id: arrow
            property color color: host.palette.buttonText
            implicitWidth: 10
            implicitHeight: 6
            width: 20
            x: host.mirrored ? host.padding : host.width - host.rightPadding
            y: host.topPadding + (host.availableHeight - height) / 2

            // A small filled triangle of our own (CLEAN_ROOM.md), eight wide and
            // four tall, in the text colour at about 72 % as qml6 shows it.
            Shape {
                x: Math.round((arrow.width - 8) / 2)
                y: Math.round((arrow.height - 4) / 2)
                width: 8
                height: 4
                ShapePath {
                    strokeWidth: -1
                    fillColor: Qt.rgba(arrow.color.r, arrow.color.g, arrow.color.b, arrow.color.a * 0.72)
                    PathSvg { path: "M 0 0 L 8 0 L 4 4 Z" }
                }
            }
        }
    }

    // A field in either case, as on Qt: one that cannot be typed in shows
    // the text of the current row and leaves a press to the combo box.
    contentItem: Component {
        TextField {
            id: field

            leftPadding: host.mirrored ? 1 : 4
            rightPadding: host.mirrored ? 4 : 1
            topPadding: 4 - host.padding
            bottomPadding: 4 - host.padding

            text: host.editable ? host.editText : host.displayText

            enabled: host.editable
            autoScroll: host.editable
            // The keys move through the rows while the popup is open.
            readOnly: host.down
            inputMethodHints: host.inputMethodHints
            validator: host.validator
            selectByMouse: host.selectTextByMouse

            font: host.font
            color: host.editable ? host.palette.text : host.palette.buttonText
            selectionColor: host.palette.highlight
            selectedTextColor: host.palette.highlightedText
            verticalAlignment: Text.AlignVCenter

            // The combo box speaks for its text.
            Accessible.ignored: !host.editable
            Accessible.name: host.Accessible.name

            // A sunken field over the button, up to the arrow's strip, with
            // a faint shadow at the top.
            background: Component {
                Rectangle {
                    visible: host.enabled && host.editable && !host.flat
                    radius: 2
                    color: host.palette.base
                    border.color: field.activeFocus ? Qt.darker(host.palette.highlight, 1.25)
                                                    : Fusion.outline(host.palette)

                    Rectangle {
                        x: 1
                        y: 1
                        width: parent.width - 3
                        height: 1
                        color: Fusion.topShadow
                    }
                }
            }
        }
    }

    // The keyboard is shown by a soft ring inside the outline, which
    // keeps its colour.
    background: Component {
        ButtonPanel {
            implicitWidth: 120
            implicitHeight: 24
            visible: !host.flat || host.down
            control: host
            pressed: host.down
            outlinesFocus: false

            Rectangle {
                readonly property color ring: Qt.darker(host.palette.highlight, 1.25)
                x: 1
                y: 1
                width: parent.width - 2
                height: parent.height - 2
                radius: 1.7
                color: "transparent"
                border.color: Qt.rgba(ring.r, ring.g, ring.b, 40 / 255)
                visible: host.activeFocus
            }
        }
    }

    // The rows, under the combo box and as wide as it is, and no taller
    // than the window has room for between the margins: more rows than
    // that scroll.
    popup: Component {
        T.Popup {
            y: host.height
            width: host.width
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
                    model: host.delegateModel
                    currentIndex: host.highlightedIndex
                    highlightMoveDuration: 0

                    ScrollIndicator.vertical: ScrollIndicator {}
                }
            }

            background: Component {
                Rectangle {
                    color: host.palette.base
                    border.color: Qt.darker(host.palette.window, 1.4)
                    radius: 2
                }
            }
        }
    }
}
