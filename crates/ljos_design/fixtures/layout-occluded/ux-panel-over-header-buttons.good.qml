import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 600; height: 400
    RowLayout {
        anchors.fill: parent; spacing: 0
        Flow {
            Layout.fillWidth: true; Layout.alignment: Qt.AlignTop; spacing: 4
            Repeater {
                model: ["Cut", "Copy", "Paste", "Undo", "Redo", "Find"]
                Button { text: modelData }
            }
        }
        Rectangle { Layout.preferredWidth: 360; Layout.fillHeight: true; color: palette.window; border.color: palette.mid }
    }
}
