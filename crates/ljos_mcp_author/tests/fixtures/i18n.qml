import QtQuick
import QtQuick.Controls

Item {
    property string word: "alpha"

    Text {
        text: "Hello"
    }
    Text {
        text: qsTr("Hello")
    }
    Text {
        text: "123"
    }
    Text {
        text: qsTr(word)
    }
    TextField {
        text: "Typed by the user"
        placeholderText: "Name"
    }
    ListModel {
        ListElement { text: "data, not a label" }
    }
    Text {
        text: "Tip"
        ToolTip.text: "A tip"
        Accessible.description: "Described"
    }
}
