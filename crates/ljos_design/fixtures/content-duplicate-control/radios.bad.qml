// expect: content-duplicate-control at 13
import QtQuick
import QtQuick.Controls

Column {
    property QtObject settings
    Slider {
        Accessible.name: qsTr("Volume")
        onMoved: settings.volume = value
    }
    SpinBox {
        Accessible.name: qsTr("Volume")
        onValueModified: settings.volume = value
    }
}
