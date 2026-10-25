import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 800; height: 400
    RowLayout {
        anchors.fill: parent; spacing: 0
        Rectangle { Layout.preferredWidth: 540; Layout.fillHeight: true; color: palette.window; border.color: palette.mid }
        Rectangle {
            Layout.fillWidth: true; Layout.fillHeight: true; Layout.minimumWidth: 420
            color: palette.base
            Label { text: "Viewer content"; anchors.centerIn: parent }
        }
    }
}
