// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

// The cell's display text; rows alternate between two shades, a selected
// or current cell has the highlight, and the current one a frame of it.
T.TableViewDelegate {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 0
    spacing: 0
    hoverEnabled: false
    focusPolicy: Qt.NoFocus
    highlighted: control.selected || control.current

    background: Component {
        Rectangle {
            border.width: control.current ? 2 : 0
            border.color: control.current ? control.palette.highlight : control.palette.windowText
            color: control.highlighted ? control.palette.highlight
                 : (control.tableView && control.tableView.alternatingRows && row % 2 !== 0)
                   ? control.palette.alternateBase : control.palette.base
        }
    }

    contentItem: Component {
        Label {
            text: model.display ?? ""
            elide: Text.ElideRight
            color: control.highlighted ? control.palette.highlightedText
                                       : control.palette.windowText
        }
    }
}
