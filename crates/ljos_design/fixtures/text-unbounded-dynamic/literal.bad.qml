// expect: text-unbounded-dynamic at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

RowLayout {
    property string userName
    width: 200
    Label {
        Layout.fillWidth: true
        text: userName
    }
}
