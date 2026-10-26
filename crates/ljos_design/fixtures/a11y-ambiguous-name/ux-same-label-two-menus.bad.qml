// expect: a11y-ambiguous-name at 18
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 160
    ToolBar {
        width: parent.width
        ToolButton {
            text: "More options"
            onClicked: fileMenu.open()
            Menu { id: fileMenu; MenuItem { text: "Rename" } MenuItem { text: "Delete" } }
        }
    }
    ToolBar {
        y: 80; width: parent.width
        ToolButton {
            text: "More options"
            onClicked: viewMenu.open()
            Menu { id: viewMenu; MenuItem { text: "Zoom in" } MenuItem { text: "Zoom out" } }
        }
    }
}
