import QtQuick
import QtQuick.Controls

Item {
    property alias value: slider.value
    implicitWidth: slider.implicitWidth
    implicitHeight: slider.implicitHeight
    Slider { id: slider; anchors.fill: parent; Accessible.name: qsTr("Level") }
}
