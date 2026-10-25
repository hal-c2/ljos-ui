// expect: color-nontext-contrast at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    TextField {
        placeholderText: "Search"
        background: Rectangle { color: palette.window; border.width: 0 }
    }
}
