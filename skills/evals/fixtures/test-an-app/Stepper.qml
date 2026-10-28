import QtQuick
import QtQuick.Controls

Row {
    id: stepper
    objectName: "stepper"
    property int value: 0
    property int maximum: 3
    spacing: 8

    Button {
        text: qsTr("Fewer")
        enabled: stepper.value > 0
        onClicked: stepper.value -= 1
    }
    Label {
        objectName: "count"
        text: stepper.value
    }
    Button {
        text: qsTr("More")
        enabled: stepper.value < stepper.maximum
        onClicked: stepper.value += 1
    }
}
