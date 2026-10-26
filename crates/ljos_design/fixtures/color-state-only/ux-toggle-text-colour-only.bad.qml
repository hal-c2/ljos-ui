// expect: color-state-only at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Button {
        text: "Bold"; checkable: true
        background: Item {}
        contentItem: Text {
            text: parent.text
            color: parent.checked ? "#1c71d8" : "#707070"
        }
    }
}
