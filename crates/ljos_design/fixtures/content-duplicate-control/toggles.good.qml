import QtQuick
import QtQuick.Controls

Column {
    property QtObject settings
    Switch {
        text: qsTr("Dark mode")
        onToggled: settings.darkMode = checked
    }
    CheckBox {
        text: qsTr("Large text")
        onToggled: settings.largeText = checked
    }
}
