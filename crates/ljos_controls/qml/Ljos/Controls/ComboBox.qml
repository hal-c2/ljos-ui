// Ljós's own style (docs/DIALECT.md §4.16): a select. Without a frame by
// default, as a picker in a toolbar is; `outline` gives it a field's edge.
// Its rows are on the overlay surface, the current one checked, and the
// list opens upward where the window has no room below.
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

    padding: 0
    leftPadding: Theme.spacing + (iconName.length > 0 ? Theme.iconSize + Theme.spacing : 0)
    rightPadding: Theme.spacing + Theme.iconSizeSmall + Theme.spacingSmall
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium
    opacity: enabled ? 1 : 0.64

    delegate: T.ItemDelegate {
        id: row
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
                    width: parent.width - Theme.spacing - Theme.iconSize
                    height: parent.height
                    text: row.text
                    font: row.font
                    color: Theme.text
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                }

                Icon {
                    x: parent.width - width
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

    contentItem: Component {
        Item {
            implicitWidth: label.implicitWidth
            implicitHeight: label.implicitHeight

            Icon {
                visible: control.iconName.length > 0
                x: -(Theme.iconSize + Theme.spacing)
                y: (parent.height - height) / 2
                name: control.iconName
                color: Theme.textMuted
            }

            Text {
                id: label
                width: parent.width
                height: parent.height
                text: control.displayText
                font: control.font
                color: Theme.text
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 120
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            color: control.down || control.hovered || control.popup.visible ? Theme.accentSurface
                 : control.outline ? Theme.surfaceRaised : "transparent"
            border.width: control.visualFocus ? 2 : control.outline ? 1 : 0
            border.color: control.visualFocus ? Theme.focus : Theme.input
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
