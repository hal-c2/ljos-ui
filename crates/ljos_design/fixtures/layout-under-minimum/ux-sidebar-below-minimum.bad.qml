// expect: layout-under-minimum at 10
// audit: size=640x400
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 640; height: 300
    Rectangle {
        width: 160; implicitWidth: 208; height: parent.height
        color: palette.window; border.color: palette.mid
        Label { x: -48; text: "A sidebar label that needs room" }
    }
}
