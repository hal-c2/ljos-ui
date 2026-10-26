// expect: content-duplicate-control at 13
import QtQuick
import QtQuick.Controls

Column {
    property QtObject settings
    Switch {
        text: qsTr("Dark mode")
        onToggled: settings.darkMode = checked
    }
    CheckBox {
        text: qsTr("Use dark colours")
        onToggled: settings.darkMode = checked
    }
}
