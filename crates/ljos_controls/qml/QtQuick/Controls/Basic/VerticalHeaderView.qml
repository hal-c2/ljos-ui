// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

// As tall as the view it heads, as wide as its sections.
T.VerticalHeaderView {
    id: control

    implicitWidth: contentWidth
    implicitHeight: syncView ? syncView.height : 0

    delegate: VerticalHeaderViewDelegate {}
}
