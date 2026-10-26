// expect: content-menu-ellipsis at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 200
    Popup { id: filterPopup; Label { text: qsTr("Filter by") } }
    Action {
        text: qsTr("Filter")
        onTriggered: filterPopup.open()
    }
}
