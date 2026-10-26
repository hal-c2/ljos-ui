import QtQuick
import QtQuick.Controls

Column {
    property QtObject settings
    RadioButton {
        text: qsTr("Light")
        onToggled: settings.scheme = text
    }
    RadioButton {
        text: qsTr("Dark")
        onToggled: settings.scheme = text
    }
}
