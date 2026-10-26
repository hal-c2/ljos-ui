import QtQuick
import QtQuick.Controls

Item {
    width: 1000; height: 400
    Label {
        x: 20; width: Math.min(parent.width - 40, 360)
        wrapMode: Text.Wrap
        text: "This release brings a faster start, a quieter fan and a new way to share a project with the people you work with. ".repeat(4)
    }
}
