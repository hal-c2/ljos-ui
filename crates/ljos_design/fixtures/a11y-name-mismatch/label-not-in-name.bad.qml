// expect: a11y-name-mismatch at 9
import QtQuick
import QtQuick.Controls

Item {
    width: 400; height: 120
    Row {
        spacing: 8
        Button {
            text: qsTr("Send")
            Accessible.name: qsTr("Submit the form")
        }
        Button {
            text: qsTr("Cancel")
        }
    }
}
