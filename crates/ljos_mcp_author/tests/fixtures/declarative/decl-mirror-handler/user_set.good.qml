import QtQuick
import QtQuick.Controls

TextField {
    property string committed: ""
    placeholderText: qsTr("Name")
    Accessible.name: qsTr("Name")
    onCommittedChanged: text = committed
}
