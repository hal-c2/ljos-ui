// Ljós's own style (docs/DIALECT.md §4.16): a tab of a TabBar, as wide as
// its text, so a strip of them reads as a row of words. The current tab is
// raised, and its text is heavier and in the text's colour; the others are
// muted, and the pointer over one lifts it a little. The weight is what
// says which is current to a reader who cannot tell the colours apart. Its
// icon (`icon.source`, or its action's) is drawn in the text's colour;
// `iconName`, one of the style's own icons, wins over it. The icon is drawn
// before the text, or over it, alone, or not at all as `display` says
// (AbstractButton.TextUnderIcon, IconOnly, TextOnly); a tab with only one
// of the two shows it whatever the display.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T
import Ljos.Design

T.TabButton {
    id: control

    property string iconName: ""
    readonly property color ink: checked ? Theme.text : Theme.textMuted

    // Its own width, so the bar does not share its width out among the
    // tabs: a tab is as wide as what it says.
    width: implicitWidth
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    horizontalPadding: Theme.spacing
    // One line is centred in the strip's height; the icon over the text is
    // taller, so the tab grows to it and keeps half a spacing above and below.
    // With only one of the two shown it is one line, whatever the display.
    verticalPadding: contentItem && contentItem.iconOverText === true ? Theme.spacing / 2 : 0
    spacing: Theme.spacingSmall
    hoverEnabled: true
    font.pixelSize: Theme.fontSizeSmall
    opacity: enabled ? 1 : 0.64

    contentItem: Component {
        Item {
            id: content
            readonly property bool hasIcon: control.iconName.length > 0 || picture.implicitWidth > 0
            readonly property bool showsIcon: control.display !== T.AbstractButton.TextOnly
                                              || control.text.length === 0
            // The icon over the text, both shown, as the grid lays them out.
            readonly property bool iconOverText: (glyph.visible || picture.visible) && label.visible
                                                 && control.display === T.AbstractButton.TextUnderIcon
            implicitWidth: row.implicitWidth
            implicitHeight: row.implicitHeight

            // One row, or the icon over the text in one column.
            Grid {
                id: row
                anchors.centerIn: parent
                // Two only when both are shown: a grid keeps the spacing
                // after a lone part in a first column of two.
                columns: (glyph.visible || picture.visible) && label.visible && !content.iconOverText ? 2 : 1
                columnSpacing: control.spacing
                rowSpacing: control.spacing
                horizontalItemAlignment: Grid.AlignHCenter
                verticalItemAlignment: Grid.AlignVCenter
                layoutDirection: control.mirrored ? Qt.RightToLeft : Qt.LeftToRight
                // The direction is the tab's own mirroring already: the grid
                // mirrored again by what the tab passes on would undo it.
                LayoutMirroring.enabled: false

                Icon {
                    id: glyph
                    objectName: "glyph"
                    visible: control.iconName.length > 0 && content.showsIcon
                    name: control.iconName
                    size: Theme.iconSizeSmall
                    color: control.ink
                }

                // Text-less, it is the icon alone, taken from the tab or the
                // action it follows, no larger than the style's icons.
                IconLabel {
                    id: picture
                    objectName: "picture"
                    visible: control.iconName.length === 0 && content.showsIcon && implicitWidth > 0
                    icon.width: Theme.iconSizeSmall
                    icon.height: Theme.iconSizeSmall
                    color: control.ink
                }

                Text {
                    id: label
                    objectName: "label"
                    visible: control.text.length > 0
                             && (control.display !== T.AbstractButton.IconOnly || !content.hasIcon)
                    // As wide as the heavier text, so a tab keeps its
                    // width as it becomes current and the strip stays put.
                    width: Math.max(implicitWidth, Math.ceil(heavy.advanceWidth))
                    horizontalAlignment: Text.AlignHCenter
                    text: control.text
                    font.family: control.font.family
                    font.pixelSize: control.font.pixelSize
                    font.weight: control.checked ? Font.DemiBold : control.font.weight
                    color: control.ink
                    elide: Text.ElideRight

                    TextMetrics {
                        id: heavy
                        font.family: control.font.family
                        font.pixelSize: control.font.pixelSize
                        font.weight: Font.DemiBold
                        text: control.text
                    }
                }
            }
            // Where the picture would not be shown it loads nothing, as a
            // button's does (Button.qml says why it is not its own source).
            Binding {
                target: picture
                property: "icon.source"
                value: ""
                when: control.iconName.length > 0
                      || control.display === T.AbstractButton.TextOnly && control.text.length > 0
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: Theme.controlHeight
            implicitHeight: Theme.controlHeight
            radius: Theme.radiusMedium
            color: control.checked ? Theme.surfaceRaised
                   : control.hovered || control.down ? Theme.surface : "transparent"
            Behavior on color {
                ColorAnimation { duration: Motion.moveSmallFrequent; easing.type: Easing.OutCubic }
            }
            // Outside the part, against the strip.
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
}
