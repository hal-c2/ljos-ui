// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.TextArea {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(contentWidth + leftPadding + rightPadding,
                            implicitBackgroundWidth + leftInset + rightInset,
                            placeholder.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(contentHeight + topPadding + bottomPadding,
                             implicitBackgroundHeight + topInset + bottomInset,
                             placeholder.implicitHeight + topPadding + bottomPadding)

    padding: 6
    leftPadding: padding + 4

    color: host.palette.text
    selectionColor: host.palette.highlight
    selectedTextColor: host.palette.highlightedText
    placeholderTextColor: host.palette.placeholderText

    // The first child: the background comes after it and lies behind.
    Text {
        id: placeholder
        x: host.leftPadding
        y: host.topPadding
        width: host.width - (host.leftPadding + host.rightPadding)
        height: host.height - (host.topPadding + host.bottomPadding)

        text: host.placeholderText
        font: host.font
        color: host.placeholderTextColor
        verticalAlignment: host.verticalAlignment
        horizontalAlignment: host.horizontalAlignment
        // A centred placeholder would sit under the cursor.
        visible: !host.length && !host.preeditText
                 && (!host.activeFocus || host.horizontalAlignment !== Qt.AlignHCenter)
        elide: Text.ElideRight
    }

    background: Component {
        TextFieldBackground {
            control: host
        }
    }
}
