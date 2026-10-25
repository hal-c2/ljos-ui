// expect: color-scheme-ignored at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    TextField {
        placeholderText: "Name"
        background: Rectangle { color: "white"; border.color: "gray" }
    }
}
