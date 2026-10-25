// expect: a11y-duplicate-name at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "notes" }
        MenuItem { text: "notes" }
    }
}
