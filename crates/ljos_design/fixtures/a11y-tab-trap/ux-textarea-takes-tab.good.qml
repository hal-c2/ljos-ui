import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 240
    Column {
        spacing: 8
        TextArea {
            width: 360; height: 120
            placeholderText: "Notes"
            KeyNavigation.priority: KeyNavigation.BeforeItem
            KeyNavigation.tab: save
        }
        Button { id: save; text: "Save" }
    }
}
