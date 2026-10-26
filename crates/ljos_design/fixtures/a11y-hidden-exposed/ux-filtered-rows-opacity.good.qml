import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 220
    property string filter: "be"
    Column {
        spacing: 4
        Repeater {
            model: ["Alpha", "Beta", "Gamma"]
            Button {
                text: modelData
                visible: modelData.toLowerCase().indexOf(filter) >= 0
            }
        }
    }
}
