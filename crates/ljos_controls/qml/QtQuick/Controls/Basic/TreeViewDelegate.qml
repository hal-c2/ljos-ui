// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.TreeViewDelegate {
    id: control

    implicitWidth: leftPadding + implicitContentWidth + rightPadding
    implicitHeight: Math.max(implicitBackgroundHeight, implicitIndicatorHeight,
                             implicitContentHeight + topPadding + bottomPadding)

    indentation: 20
    leftMargin: 5
    rightMargin: 5
    spacing: 5
    topPadding: 0
    bottomPadding: 0

    // The text starts after the levels above the row and the indicator.
    leftPadding: isTreeNode ? leftMargin + depth * indentation + indicator.width + spacing
                            : leftMargin
    rightPadding: rightMargin

    highlighted: control.selected || control.current

    // A row with rows under it: an arrow that expands and collapses it.
    indicator: Component {
        Item {
            implicitWidth: 20
            implicitHeight: 24
            x: control.leftMargin + control.depth * control.indentation
            visible: control.isTreeNode && control.hasChildren
            Text {
                anchors.centerIn: parent
                text: control.expanded ? "▾" : "▸"
                color: control.highlighted ? control.palette.highlightedText
                                           : control.palette.buttonText
            }
            MouseArea {
                anchors.fill: parent
                onClicked: control.treeView.toggleExpanded(row)
            }
        }
    }

    contentItem: Component {
        Label {
            text: model.display
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            color: control.highlighted ? control.palette.highlightedText
                                       : control.palette.buttonText
        }
    }

    // Rows alternate between two shades; a current or selected row has
    // the highlight.
    background: Component {
        Rectangle {
            color: control.highlighted ? control.palette.highlight
                 : (control.treeView && control.treeView.alternatingRows && row % 2 !== 0)
                   ? control.palette.alternateBase : control.palette.base
        }
    }
}
