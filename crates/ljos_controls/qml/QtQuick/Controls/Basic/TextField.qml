// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.TextField {
    id: control

    // A background with a width of its own decides; without one the field
    // is as wide as what it shows.
    implicitWidth: implicitBackgroundWidth + leftInset + rightInset
                   || Math.max(contentWidth, placeholder.implicitWidth) + leftPadding + rightPadding
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding,
                             placeholder.implicitHeight + topPadding + bottomPadding)

    padding: 6
    leftPadding: padding + 4

    color: control.palette.text
    selectionColor: control.palette.highlight
    selectedTextColor: control.palette.highlightedText
    placeholderTextColor: control.palette.placeholderText
    verticalAlignment: TextInput.AlignVCenter

    // The first child: the background comes after it and lies behind.
    Text {
        id: placeholder
        x: control.leftPadding
        y: control.topPadding
        width: control.width - (control.leftPadding + control.rightPadding)
        height: control.height - (control.topPadding + control.bottomPadding)

        text: control.placeholderText
        font: control.font
        color: control.placeholderTextColor
        verticalAlignment: control.verticalAlignment
        horizontalAlignment: control.horizontalAlignment
        // A centred placeholder would sit under the cursor.
        visible: !control.length && !control.preeditText
                 && (!control.activeFocus || control.horizontalAlignment !== Qt.AlignHCenter)
        elide: Text.ElideRight
    }

    background: Rectangle {
        implicitWidth: 200
        implicitHeight: 40
        border.width: control.activeFocus ? 2 : 1
        color: control.palette.base
        border.color: control.activeFocus ? control.palette.highlight : control.palette.mid
    }
}
