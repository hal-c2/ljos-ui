import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
Rectangle {
    width: 640; height: 480
    color: "white"
    RowLayout {
        width: 300; height: 100
        ColumnLayout {
            Layout.fillWidth: true
            RowLayout {
                Flow {
                    Layout.fillWidth: true
                    spacing: 4
                    Button { implicitWidth: 80; text: qsTr("Open") }
                    Button { implicitWidth: 80; text: qsTr("Save") }
                }
                Button { implicitWidth: 80; text: qsTr("Send") }
            }
        }
        Rectangle { Layout.preferredWidth: 120; Layout.fillHeight: true; color: "#eeeeee" }
    }
}
