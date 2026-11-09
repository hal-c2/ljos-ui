// Ljós's own style (docs/DIALECT.md §4.16): a select. Without a frame by
// default, as a picker in a toolbar is; `outline` gives it a field's edge.
// Its rows are on the overlay surface, the current one checked, and the
// list opens upward where the window has no room below. An `editable` one
// is a field, framed as `outline` gives, with a text field's edit menu.
import QtQuick
import QtQuick.Effects
import QtQuick.Templates as T

T.ComboBox {
    id: control

    property bool outline: false
    // Rows that cannot be chosen; the keys still reach them, so a handler
    // of `activated` should refuse them too.
    property var disabledRows: []
    property bool markCurrent: true
    property string iconName: ""

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             Math.max(implicitContentHeight, implicitIndicatorHeight)
                             + topPadding + bottomPadding)

    // The icon's room is at the start and the chevron's at the end, which
    // mirroring swaps.
    readonly property real startSpace: Theme.spacing + (iconName.length > 0 ? Theme.iconSize + Theme.spacing : 0)
    readonly property real endSpace: Theme.spacing + Theme.iconSizeSmall + Theme.spacingSmall
    padding: 0
    leftPadding: mirrored ? endSpace : startSpace
    rightPadding: mirrored ? startSpace : endSpace
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium
    opacity: enabled ? 1 : 0.64

    delegate: T.ItemDelegate {
        id: row
        objectName: "row" + index
        width: ListView.view ? ListView.view.width : control.width
        implicitHeight: Theme.controlHeight
        text: control.textAt(index)
        readonly property bool chosen: control.markCurrent && index === control.currentIndex
        enabled: control.disabledRows.indexOf(index) < 0
        highlighted: control.highlightedIndex === index
        hoverEnabled: control.hoverEnabled
        leftPadding: Theme.spacing
        rightPadding: Theme.spacing
        font: control.font
        opacity: enabled ? 1 : 0.64

        Accessible.checkable: control.markCurrent
        Accessible.checked: chosen

        contentItem: Component {
            Item {
                implicitWidth: label.implicitWidth + Theme.spacing + Theme.iconSize
                implicitHeight: label.implicitHeight

                Text {
                    id: label
                    objectName: "rowLabel"
                    x: control.mirrored ? Theme.spacing + Theme.iconSize : 0
                    width: parent.width - Theme.spacing - Theme.iconSize
                    height: parent.height
                    text: row.text
                    font: row.font
                    color: Theme.text
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                    horizontalAlignment: control.mirrored ? Text.AlignRight : Text.AlignLeft
                }

                Icon {
                    objectName: "check"
                    x: control.mirrored ? 0 : parent.width - width
                    y: (parent.height - height) / 2
                    visible: row.chosen
                    name: "check"
                    color: Theme.focus
                }
            }
        }

        background: Component {
            Rectangle {
                radius: Math.max(0, Theme.radiusLarge - Theme.spacingSmall)
                color: row.highlighted || row.hovered ? Theme.accentSurface : "transparent"
                // The row Return would choose: its fill alone is too near
                // the popup's to tell it by, and the focus colour is 3:1 on
                // both. The check stays on the current row.
                border.width: row.highlighted ? 2 : 0
                border.color: Theme.focus
            }
        }
    }

    indicator: Component {
        Icon {
            x: control.mirrored ? Theme.spacing : control.width - width - Theme.spacing
            y: (control.height - height) / 2
            name: "chevron-down"
            size: Theme.iconSizeSmall
            color: Theme.iconMuted
        }
    }

    // A field, as every style's combo box has, so an editable one is typed
    // in with the field's keys, validator and edit menu (the style's own
    // TextField gives the style's own menu). One that is not editable shows
    // its row in a label instead, which can be cut short where it does not
    // fit, as a field's text cannot.
    contentItem: Component {
        TextField {
            id: field
            implicitWidth: label.implicitWidth
            implicitHeight: label.implicitHeight
            padding: 0
            background: null
            // The combo box is dimmed when it is disabled; the field is
            // disabled whenever the combo box is not editable.
            opacity: 1

            text: control.editable ? control.editText : ""
            enabled: control.editable
            autoScroll: control.editable
            // The keys move through the rows while the popup is open.
            readOnly: control.down
            inputMethodHints: control.inputMethodHints
            validator: control.validator
            selectByMouse: control.selectTextByMouse
            font: control.font
            horizontalAlignment: control.mirrored ? TextInput.AlignRight : TextInput.AlignLeft

            // The combo box speaks for its text.
            Accessible.ignored: !control.editable
            Accessible.name: control.Accessible.name

            Icon {
                objectName: "glyph"
                visible: control.iconName.length > 0
                x: control.mirrored ? parent.width + Theme.spacing : -(Theme.iconSize + Theme.spacing)
                y: (parent.height - height) / 2
                name: control.iconName
                color: Theme.textMuted
            }

            Text {
                id: label
                objectName: "label"
                visible: !control.editable
                width: parent.width
                height: parent.height
                text: control.displayText
                font: control.font
                color: Theme.text
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: control.mirrored ? Text.AlignRight : Text.AlignLeft
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 120
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            // A field has the keyboard however it got it, so an editable one
            // shows it whenever it has it, as a text field does.
            readonly property bool ring: control.visualFocus || control.editable && control.activeFocus
            readonly property bool framed: control.outline || control.editable
            color: control.down || control.hovered || control.popup.visible ? Theme.accentSurface
                 : framed ? Theme.surfaceRaised : "transparent"
            border.width: ring ? 2 : framed ? 1 : 0
            border.color: ring ? Theme.focus : Theme.input
        }
    }

    popup: Component {
        T.Popup {
            // Chosen as it opens: the combo box does not move while it is open.
            property bool upward: false
            y: upward ? -implicitHeight - Theme.spacingSmall : control.height + Theme.spacingSmall
            width: Math.max(control.width, 160)
            implicitHeight: Math.min(320, contentItem.implicitHeight + topPadding + bottomPadding)
            padding: Theme.spacingSmall
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
            onAboutToShow: upward = control.Window.window !== null
                                    && control.mapToItem(null, 0, control.height).y
                                       + implicitHeight + Theme.spacingSmall > control.Window.height

            enter: Transition {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 120; easing.type: Easing.OutCubic }
            }
            exit: null

            contentItem: Component {
                ListView {
                    clip: true
                    implicitHeight: contentHeight
                    model: control.delegateModel
                    currentIndex: control.highlightedIndex
                    highlightMoveDuration: 0
                    boundsBehavior: Flickable.StopAtBounds
                }
            }

            background: Component {
                Item {
                    RectangularShadow {
                        x: frame.x
                        y: frame.y
                        width: frame.width
                        height: frame.height
                        radius: frame.radius
                        offset.y: 8
                        blur: 24
                        color: Qt.rgba(0, 0, 0, Theme.dark ? 0.5 : 0.12)
                    }

                    Rectangle {
                        id: frame
                        width: parent.width
                        height: parent.height
                        radius: Theme.radiusLarge
                        color: Theme.surfaceOverlay
                        border.width: 1
                        border.color: Theme.border
                    }
                }
            }
        }
    }
}
