// expect: text-elided-hidden at 8
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 80
    Text {
        width: 120
        color: palette.windowText
        elide: Text.ElideRight
        text: "Quarterly planning notes for the whole team"
    }
}
