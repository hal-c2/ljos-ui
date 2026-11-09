// Ljós's own style (docs/DIALECT.md §4.16): a row of a menu, with an
// optional icon (`iconName`, one of the style's own, wins over
// `icon.source` and its action's), the shortcut that runs it muted at the end, and a
// chevron where it opens a submenu.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.MenuItem {
    id: control

    property bool destructive: false
    property string iconName: ""
    // The keys that run it, as a person reads them ("Ctrl+N").
    property string shortcut: ""
    // Keeps the icon's column when this row has none, so its label lines
    // up with the rows that do; on wherever another row of the menu has one.
    property bool iconSpace: menu !== null && menu.hasIcons === true
    readonly property color ink: destructive ? Theme.errorForeground : Theme.text

    // A picture larger than the icons' column is made no larger.
    icon.width: Theme.iconSize
    icon.height: Theme.iconSize

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             Theme.controlHeight)

    padding: 0
    leftPadding: Theme.spacing
    rightPadding: Theme.spacing
    spacing: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    opacity: enabled ? 1 : 0.64
    // The chevron is in the row; the template's arrow would lie on it.
    arrow: null

    indicator: Component {
        Icon {
            x: control.mirrored ? control.leftPadding : control.width - width - control.rightPadding
            y: (control.height - height) / 2
            visible: control.checked
            name: "check"
            color: Theme.focus
        }
    }

    contentItem: Component {
        Item {
            // The style's own icon, else the row's or its action's.
            readonly property Item mark: glyph.visible ? glyph : picture.visible ? picture : null
            readonly property real gutter: mark || control.iconSpace
                                           ? iconSize + control.spacing : 0
            readonly property real end: (keys.visible ? keys.implicitWidth + control.spacing : 0)
                                        + (more.visible ? more.width + control.spacing : 0)
                                        + (control.checkable ? Theme.iconSize + control.spacing : 0)
            readonly property real iconSize: Theme.iconSize
            implicitWidth: gutter + label.implicitWidth + end
            implicitHeight: Math.max(label.implicitHeight, Theme.iconSize)

            // Mirroring moves anchors only: the icon's column is at the
            // start of the row, and the shortcut, the check's room and the
            // chevron at its end, on either side.
            Icon {
                id: glyph
                objectName: "glyph"
                visible: control.iconName.length > 0
                x: control.mirrored ? parent.width - width : 0
                y: (parent.height - height) / 2
                name: control.iconName
                color: control.destructive ? Theme.errorForeground : Theme.textMuted
            }

            // Text-less, it is the icon alone, centred in the icons' column.
            IconLabel {
                id: picture
                objectName: "picture"
                visible: !glyph.visible && implicitWidth > 0
                x: control.mirrored ? parent.width - parent.iconSize + (parent.iconSize - width) / 2
                                    : (parent.iconSize - width) / 2
                y: (parent.height - height) / 2
                color: control.destructive ? Theme.errorForeground : Theme.textMuted
            }

            // The text as its mnemonic says, a submenu's title too; the
            // keys read `control.text` as written. Its own icon is none:
            // the one in the icons' column is drawn beside it.
            IconLabel {
                id: label
                objectName: "label"
                x: control.mirrored ? parent.end : parent.gutter
                width: parent.width - parent.gutter - parent.end
                height: parent.height
                icon.name: ""
                icon.source: ""
                text: control.text
                font: control.font
                color: control.ink
                mirrored: control.mirrored
                // Left is the start: a mirrored label sets it at the right.
                alignment: Qt.AlignLeft | Qt.AlignVCenter
            }

            Text {
                id: keys
                objectName: "keys"
                visible: control.shortcut.length > 0 && control.subMenu === null
                readonly property real checkSpace: control.checkable ? Theme.iconSize + control.spacing : 0
                x: control.mirrored ? checkSpace : parent.width - width - checkSpace
                height: parent.height
                text: control.shortcut
                font.family: control.font.family
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.textMuted
                verticalAlignment: Text.AlignVCenter
            }

            Icon {
                id: more
                objectName: "more"
                visible: control.subMenu !== null
                // Past the check's room, as the shortcut is: the menu's
                // delegate draws its submenus' rows too, checked or not.
                readonly property real checkSpace: control.checkable ? Theme.iconSize + control.spacing : 0
                x: control.mirrored ? checkSpace : parent.width - width - checkSpace
                y: (parent.height - height) / 2
                name: control.mirrored ? "chevron-left" : "chevron-right"
                size: Theme.iconSizeSmall
                color: Theme.textMuted
            }
        }
    }

    // Nested in the menu's corner, its padding in.
    background: Component {
        Rectangle {
            implicitWidth: 200
            implicitHeight: Theme.controlHeight
            radius: Math.max(0, Theme.radiusLarge - Theme.spacingSmall)
            color: control.highlighted || control.hovered || control.visualFocus
                   ? Theme.accentSurface : "transparent"
            // The row the keys are on: its fill alone is too near the
            // menu's to tell it by, and the focus colour is 3:1 on both.
            border.width: control.highlighted || control.visualFocus ? 2 : 0
            border.color: Theme.focus
        }
    }
}
