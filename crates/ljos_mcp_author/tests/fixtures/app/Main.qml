import QtQuick

Item {
    Card {
        title: "One"
        label: "two"
        onOpened: reason => console.log(reason)
        Text {
            text: parent.title
            Accessible.name: text
        }
    }
    Card {
        titel: "typo"
        onClosed: console.log("never")
    }
    Text {
        text: missingName
        Accessible.name: text
    }
    Gizmo {}
}
