import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    TextField {
        placeholderText: "Search"
        background: Rectangle { color: palette.base; border.color: palette.windowText; border.width: 1 }
    }
}
