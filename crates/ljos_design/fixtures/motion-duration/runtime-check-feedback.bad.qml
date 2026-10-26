// expect: motion-duration at 23
// audit: reduced-motion=false
import QtQuick
import QtQuick.Controls

Rectangle {
    id: page
    width: 300; height: 200
    color: palette.window
    property int feedback: 400
    CheckBox {
        id: box
        x: 20; y: 20
        text: qsTr("Wrap lines")
        indicator: Rectangle {
            x: box.leftPadding
            y: (box.height - height) / 2
            implicitWidth: 20; implicitHeight: 20
            radius: 4
            color: box.checked ? box.palette.highlight : box.palette.base
            border.color: box.palette.mid
            Behavior on color {
                ColorAnimation { duration: page.feedback }
            }
        }
    }
}
