import QtQuick

Item {
    width: 200
    height: 200
    Rectangle {
        id: panel
        width: 100
        height: 100
        visible: false
    }
    states: State {
        name: "open"
        PropertyChanges {
            panel.visible: true
        }
    }
    transitions: Transition {
        to: ""
        NumberAnimation {
            target: panel
            property: "opacity"
            duration: 200
            easing.type: Easing.InExpo
        }
    }
}
