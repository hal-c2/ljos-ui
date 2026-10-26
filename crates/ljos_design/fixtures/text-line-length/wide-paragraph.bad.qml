// expect: text-line-length at 9
import QtQuick
import QtQuick.Controls

Item {
    width: 1000; height: 400
    // The paragraph runs the whole window wide: well over 80
    // characters a line.
    Label {
        x: 20; width: 960
        wrapMode: Text.Wrap
        text: "This release brings a faster start, a quieter fan and a new way to share a project with the people you work with. ".repeat(4)
    }
}
