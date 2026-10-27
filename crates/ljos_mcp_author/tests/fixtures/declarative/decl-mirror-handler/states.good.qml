import QtQuick

Item {
    id: root
    property real level: 0
    property real shown: 0
    onLevelChanged: shown = level
    states: State { name: "full"; PropertyChanges { root.shown: 1 } }
}
