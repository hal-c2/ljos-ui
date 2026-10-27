import QtQuick
import QtQml.Models

// World.qml with the bindings and handlers of the cases taken out: what is
// left for a back end that compiled them ahead of time to fill in.
Item {
    id: root
    objectName: "root"

    property real done: 3
    property real total: 10
    property bool stopMode: false
    property string lastMode: ""
    property string status: "todo"
    property string runStatus: "proposed"
    property var changes: [false, true, false]
    property bool health: false
    property var selected: []
    property int ran: -1
    property int cursor: 0
    property int depth: 0
    property bool accepted: false
    property var items: []
    property int syncs: 0

    signal clicked()
    signal dispatched(string action)
    signal picked(string value)
    signal keyPressed(int key, int modifiers)

    property real progress
    property string statusText
    property string runText

    Item {
        id: bar
        objectName: "bar"
        width: 240
    }

    Item {
        id: editor
        objectName: "editor"
        signal submitted(string mode)
    }

    Item {
        id: field
        objectName: "field"
        property string text: ""
    }

    ListModel {
        id: cards
        objectName: "cards"
    }
}
