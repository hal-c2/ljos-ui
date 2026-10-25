import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 640; height: 300
    Rectangle {
        implicitWidth: 208; width: implicitWidth; height: parent.height
        color: palette.window; border.color: palette.mid
        Label { x: 8; text: "A sidebar label that needs room"; width: parent.width - 16; wrapMode: Text.Wrap }
    }
}
