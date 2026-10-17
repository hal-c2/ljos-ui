// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

// As long as the view it heads, as deep as its sections.
T.HorizontalHeaderView {
    id: control

    implicitWidth: syncView ? syncView.width : 0
    implicitHeight: contentHeight

    delegate: HorizontalHeaderViewDelegate {}
}
