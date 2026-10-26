// expect: a11y-ignored-focusable at 9
import QtQuick
import QtQuick.Controls

Item {
    width: 400; height: 120
    Row {
        spacing: 8
        Rectangle {
            width: 120; height: 40; color: "#dddddd"
            activeFocusOnTab: true
            Accessible.ignored: true
        }
        Button { text: qsTr("Next") }
    }
}
