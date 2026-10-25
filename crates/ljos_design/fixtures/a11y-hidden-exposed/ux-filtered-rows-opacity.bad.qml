// expect: a11y-hidden-exposed at 15
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 220
    property string filter: "a"
    Column {
        spacing: 4
        Repeater {
            model: ["Alpha", "Beta", "Gamma"]
            Button {
                text: modelData
                opacity: modelData.toLowerCase().indexOf(filter) >= 0 ? 1 : 0
            }
        }
    }
}
